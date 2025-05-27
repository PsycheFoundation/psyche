use allowlist::Allowlist;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use download_manager::{DownloadManager, DownloadManagerEvent, DownloadUpdate};
use futures_util::StreamExt;
use iroh::endpoint::RemoteInfo;
use iroh_blobs::{
    downloader::ConcurrencyLimits,
    net_protocol::{Blobs, DownloadMode},
    rpc::client::blobs::DownloadOptions,
    store::mem::Store,
    util::SetTagOption,
    BlobFormat,
};
use iroh_gossip::{
    net::{Gossip, GossipEvent, GossipReceiver, GossipSender},
    proto::{HyparviewConfig, PlumtreeConfig},
};
use p2p_model_sharing::{
    ModelConfigSharingMessage, ParameterSharingMessage, MODEL_REQUEST_TIMEOUT_SECS,
};
use router::Router;
use state::State;
use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    hash::{DefaultHasher, Hash as _, Hasher},
    marker::PhantomData,
    net::{IpAddr, Ipv4Addr, SocketAddrV4},
    ops::Sub,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::{mpsc::UnboundedReceiver, oneshot, Mutex},
};
use tokio::{
    sync::mpsc,
    time::{interval, Interval},
};
use tokio_util::{sync::CancellationToken, time::FutureExt};
use tracing::{debug, error, info, trace, warn};
use util::{fmt_relay_mode, gossip_topic};

pub use ed25519::Signature;
pub use iroh::{endpoint::ConnectionType, NodeAddr, NodeId, RelayMode};
pub use iroh_blobs::{ticket::BlobTicket, Hash};

pub mod allowlist;
mod authenticable_identity;
mod download_manager;
mod local_discovery;
mod p2p_model_sharing;
mod peer_list;
mod router;
mod serde;
mod serializable_kind;
mod serializable_tensor;
mod serialized_distro;
mod signed_message;
mod state;
mod tcp;
mod tui;
mod util;

pub use authenticable_identity::{raw_p2p_verify, AuthenticatableIdentity, FromSignedBytesError};
pub use download_manager::{DownloadComplete, DownloadFailed, DownloadType, TransmittableDownload};
use iroh::defaults::DEFAULT_STUN_PORT;
pub use iroh::{Endpoint, PublicKey, SecretKey};
use iroh_relay::{RelayMap, RelayNode, RelayQuicConfig};
pub use p2p_model_sharing::{
    ModelRequestType, ModelSharing, SharableModel, SharableModelError, TransmittableModelConfig,
    ALPN,
};
pub use peer_list::PeerList;
pub use serde::Networkable;
pub use serialized_distro::{
    distro_results_from_reader, distro_results_to_bytes, SerializeDistroResultError,
    SerializedDistroResult, TransmittableDistroResult,
};
pub use signed_message::SignedMessage;
pub use tcp::{ClientNotification, TcpClient, TcpServer};
pub use tui::{NetworkTUIState, NetworkTui};
use url::Url;
pub use util::fmt_bytes;

const USE_RELAY_HOSTNAME: &str = "use1-1.relay.psyche.iroh.link";
const USW_RELAY_HOSTNAME: &str = "usw1-1.relay.psyche.iroh.link";
const EUC_RELAY_HOSTNAME: &str = "euc1-1.relay.psyche.iroh.link";

/// How should this node discover other nodes?
///
/// In almost all cases, you want "N0", for over-the-internet communication.
/// For running tests, you might want Local, since Iroh's relay nodes have a rate limit per-ip.
#[derive(Debug, Clone, Copy)]
pub enum DiscoveryMode {
    Local,
    N0,
}
pub struct NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    router: Arc<Router>,
    blobs: Blobs<Store>,
    state: State,
    gossip_tx: GossipSender,
    gossip_rx: GossipReceiver,
    rx_model_parameter_req: UnboundedReceiver<ParameterSharingMessage>,
    rx_model_config_req: UnboundedReceiver<ModelConfigSharingMessage>,
    download_manager: DownloadManager<Download>,
    _broadcast_message: PhantomData<BroadcastMessage>,
    _download: PhantomData<Download>,
    update_stats_interval: Interval,
}

impl<B, D> Debug for NetworkConnection<B, D>
where
    B: Networkable,
    D: Networkable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConnection")
            .field("router", &self.router)
            .field("blobs", &self.blobs)
            .field("gossip_tx", &self.gossip_tx)
            .field("gossip_rx", &self.gossip_rx)
            .field("state", &self.state)
            .field("download_manager", &self.download_manager)
            .field("update_stats_interval", &self.update_stats_interval)
            .finish()
    }
}

impl<BroadcastMessage, Download> NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    #[allow(clippy::too_many_arguments)]
    pub async fn init<A: Allowlist + 'static + Send>(
        run_id: &str,
        port: Option<u16>,
        interface: Option<String>,
        relay_mode: RelayMode,
        discovery_mode: DiscoveryMode,
        bootstrap_peers: Vec<NodeAddr>,
        secret_key: Option<SecretKey>,
        allowlist: A,
        max_concurrent_downloads: usize,
    ) -> Result<Self> {
        let secret_key = match secret_key {
            None => SecretKey::generate(&mut rand::rngs::OsRng),
            Some(key) => key,
        };

        let public_key = secret_key.public();

        debug!("Using relay servers: {}", fmt_relay_mode(&relay_mode));

        let ipv4 = if let Some(if_name) = interface {
            let (wildcard, if_name) = if if_name.ends_with("*") {
                (true, if_name[..if_name.len() - 1].to_string())
            } else {
                (false, if_name)
            };
            let iface_ip = get_if_addrs::get_if_addrs()
                .unwrap()
                .iter()
                .find_map(|interface| {
                    (if wildcard {
                        interface.name.starts_with(&if_name)
                    } else {
                        interface.name == if_name
                    } && interface.ip().is_ipv4())
                    .then_some(interface.ip())
                });
            let IpAddr::V4(v4) =
                iface_ip.ok_or(anyhow!("no interface with name \"{if_name}\" found."))?
            else {
                unreachable!("checked in earlier if. should not be possible.")
            };
            v4
        } else {
            Ipv4Addr::new(0, 0, 0, 0)
        };

        let endpoint = {
            let endpoint = Endpoint::builder()
                .secret_key(secret_key)
                .relay_mode(RelayMode::Custom(psyche_relay_map()))
                .bind_addr_v4(SocketAddrV4::new(ipv4, port.unwrap_or(0)));

            let e = match discovery_mode {
                DiscoveryMode::Local => endpoint.discovery(Box::new(
                    local_discovery::LocalTestDiscovery::new(public_key),
                )),
                DiscoveryMode::N0 => endpoint.discovery_n0(),
            };

            e.bind().await?
        };

        let node_addr = endpoint.node_addr().await?;

        info!("Our node addr: {}", node_addr.node_id);
        info!("Our join ticket: {}", PeerList(vec![node_addr]));

        trace!("creating blobs...");
        let blobs = Blobs::memory()
            .concurrency_limits(ConcurrencyLimits {
                max_concurrent_requests_per_node: 1,
                max_concurrent_requests: max_concurrent_downloads,
                max_open_connections: 512,
                max_concurrent_dials_per_hash: 2,
            })
            .build(&endpoint);
        trace!("blobs created!");

        trace!("creating gossip...");
        let gossip = Gossip::builder()
            .max_message_size(4096)
            .membership_config(HyparviewConfig {
                active_view_capacity: 8,
                shuffle_interval: Duration::from_secs(30),
                neighbor_request_timeout: Duration::from_secs(2),
                ..HyparviewConfig::default()
            })
            .broadcast_config(PlumtreeConfig {
                graft_timeout_2: Duration::from_millis(200),
                message_cache_retention: Duration::from_secs(60),
                message_id_retention: Duration::from_secs(2 * 60),
                ..PlumtreeConfig::default()
            })
            .spawn(endpoint.clone())
            .await?;
        trace!("gossip created!");

        trace!("creating model parameter sharing...");
        let (tx_model_parameter_req, rx_model_parameter_req) = mpsc::unbounded_channel();
        let (tx_model_config_req, rx_model_config_req) = mpsc::unbounded_channel();
        let model_parameter_sharing =
            ModelSharing::new(tx_model_parameter_req, tx_model_config_req);
        trace!("model parameter sharing created!");

        trace!("creating router...");
        let router = Arc::new(
            Router::spawn(
                endpoint,
                gossip.clone(),
                blobs.clone(),
                model_parameter_sharing.clone(),
                allowlist,
            )
            .await?,
        );
        trace!("router created!");

        // add any bootstrap peers
        {
            if bootstrap_peers.is_empty() {
                info!("Waiting for peers to join us...");
            } else {
                info!("Trying to connect to {} peers...", bootstrap_peers.len());
                // add the peer addrs from the ticket to our endpoint's addressbook so that they can be dialed
                for peer in &bootstrap_peers {
                    router.endpoint().add_node_addr(peer.clone())?;
                }
            };
        }

        let (gossip_tx, gossip_rx) = gossip
            .subscribe(
                gossip_topic(run_id),
                bootstrap_peers.iter().map(|p| p.node_id).collect(),
            )?
            .split();
        info!("Connected!");

        // if this is not 1s, the bandwidth chart will be wrong.
        let update_stats_interval = interval(Duration::from_secs(1));

        Ok(Self {
            blobs,
            gossip_rx,
            gossip_tx,
            rx_model_parameter_req,
            rx_model_config_req,

            router,

            update_stats_interval,
            state: State::new(15),
            download_manager: DownloadManager::new()?,
            _broadcast_message: Default::default(),
            _download: Default::default(),
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.router.shutdown().await
    }

    pub fn node_id(&self) -> NodeId {
        self.router.endpoint().node_id()
    }

    /// Don't call this often / with many peers!
    /// It can force disconnection of other gossip peers if we have too many.
    pub fn add_peers(&mut self, peers: Vec<NodeId>) {
        let peer_list = peers
            .iter()
            .map(|n| n.fmt_short())
            .collect::<Vec<_>>()
            .join(",");
        debug!(name: "gossip_join_peers", peers=peer_list);

        let gossip_tx = self.gossip_tx.clone();
        let node_id = self.router.endpoint().node_id();
        tokio::task::spawn(async move {
            if let Err(err) = gossip_tx
                .join_peers(peers.into_iter().filter(|p| p != &node_id).collect())
                .await
            {
                error!("Failed to join new gossip peers: {err:?}");
            }
        });
    }

    pub async fn broadcast(&mut self, message: &BroadcastMessage) -> Result<()> {
        let encoded_message =
            SignedMessage::sign_and_encode(self.router.endpoint().secret_key(), message)?;
        let message_hash = hash_bytes(&encoded_message);
        debug!(
            name: "gossip_broadcast",
            message_hash = message_hash,
            "broadcasted gossip message with hash {message_hash}: {:?}",
            message
        );
        Ok(self.gossip_tx.broadcast(encoded_message).await?)
    }

    pub async fn start_download(
        &mut self,
        ticket: BlobTicket,
        tag: u32,
        download_type: DownloadType,
    ) -> Result<()> {
        let provider_node_id = ticket.node_addr().clone();
        let additional_peers_to_try = match download_type.clone() {
            DownloadType::DistroResult(peers) => peers,
            DownloadType::ModelSharing(_) => vec![],
        };
        let mut progress = self
            .blobs
            .client()
            .download_with_opts(
                ticket.hash(),
                DownloadOptions {
                    format: BlobFormat::Raw,
                    nodes: std::iter::once(provider_node_id)
                        .chain(additional_peers_to_try.iter().cloned())
                        .collect(),
                    tag: SetTagOption::Auto,
                    mode: DownloadMode::Queued,
                },
            )
            .await?;

        let hash = ticket.hash();
        self.state.currently_sharing_blobs.insert(hash);
        self.state.blob_tags.insert((tag, hash));
        debug!(name: "blob_download_start", hash = hash.fmt_short(), "started downloading blob {}", hash.fmt_short());

        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                match progress.next().await {
                    None => break,
                    Some(val) => {
                        if let Err(err) = tx.send(val) {
                            panic!("Failed to send download progress: {err:?} {:?}", err.0);
                        }
                    }
                }
            }
        });

        self.download_manager.add(ticket, tag, rx, download_type);

        Ok(())
    }

    pub async fn add_downloadable(&mut self, data: Download, tag: u32) -> Result<BlobTicket> {
        let blob_res = self
            .blobs
            .client()
            .add_bytes(postcard::to_allocvec(&data)?)
            .await?;
        let addr = self.router.endpoint().node_addr().await?;
        let blob_ticket = BlobTicket::new(addr, blob_res.hash, blob_res.format)?;

        debug!(
            name: "blob_upload",
            hash = blob_res.hash.fmt_short(),
            size = blob_res.size,
            "blob added for upload with hash {} and size {}",
            blob_res.hash.fmt_short(),
            blob_res.size
        );

        let hash = blob_ticket.hash();
        self.state.currently_sharing_blobs.insert(hash);
        self.state.blob_tags.insert((tag, hash));

        Ok(blob_ticket)
    }

    // TODO: there must be some clever way to do this using Iroh-blobs' built-in tagging system & GC.
    pub fn remove_blobs_with_tag_less_than(&mut self, tag: u32) {
        self.state.blob_tags.retain(|(t, _)| *t >= tag);
        self.cleanup_untagged_blogs();
    }
    pub fn cleanup_untagged_blogs(&mut self) {
        let expired_blobs: Vec<_> = self
            .state
            .currently_sharing_blobs
            .iter()
            .filter(|a| !self.state.blob_tags.iter().any(|(_, b)| *a == b))
            .copied()
            .collect();
        for hash in expired_blobs.iter() {
            self.state.currently_sharing_blobs.remove(hash);
        }
        let client = self.blobs.client().clone();
        tokio::task::spawn(async move {
            for hash in expired_blobs {
                if let Err(err) = client.delete_blob(hash).await {
                    warn!("error deleting blob {hash}: {err}")
                }
            }
        });
    }

    // TODO: there must be some clever way to do this using Iroh-blobs' built-in tagging system & GC.
    pub fn remove_blobs_with_tag_equal_to(&mut self, tag: u32) {
        self.state.blob_tags.retain(|(t, _)| *t != tag);
        self.cleanup_untagged_blogs();
    }

    pub async fn node_addr(&self) -> Result<NodeAddr> {
        self.router.endpoint().node_addr().await
    }

    pub async fn join_ticket(&self) -> Result<String> {
        let me = self.router.endpoint().node_addr().await?;
        Ok(PeerList(vec![me]).to_string())
    }

    /// RemoteInfo and bandwidth in bytes/s for a node
    pub fn remote_infos(&self) -> Vec<(RemoteInfo, f64)> {
        self.router
            .endpoint()
            .remote_info_iter()
            .map(|node_info| {
                let bandwidth = self
                    .state
                    .bandwidth_tracker
                    .get_bandwidth_by_node(&node_info.node_id)
                    .unwrap_or_default();
                (node_info, bandwidth)
            })
            .collect()
    }

    pub async fn poll_next(&mut self) -> Result<Option<NetworkEvent<BroadcastMessage, Download>>> {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = self.gossip_rx.next() => {
                match parse_gossip_event(event.map_err(|ee| ee.into()), &self.gossip_rx) {
                    Some(result) => Ok(Some(NetworkEvent::MessageReceived(result))),
                    None => Ok(None),
                }
            }
            update = self.download_manager.poll_next() => {
                match update {
                    Some(DownloadManagerEvent::Complete(result)) => {
                        Ok(Some(NetworkEvent::DownloadComplete(result)))
                    }
                    Some(DownloadManagerEvent::Update(update)) => {
                        Ok(self.on_download_update(update))
                    },
                    Some(DownloadManagerEvent::Failed(result)) => {
                        self.state.download_progesses.remove(&result.blob_ticket.hash());
                        Ok(Some(NetworkEvent::DownloadFailed(result)))
                    }
                    None => Ok(None),
                }
            }
            Some(ParameterSharingMessage::Get(parameter_name, protocol_req_tx)) = self.rx_model_parameter_req.recv() => {
                Ok(Some(NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx)))
            }
            Some(ModelConfigSharingMessage::Get(protocol_req_tx)) = self.rx_model_config_req.recv() => {
                Ok(Some(NetworkEvent::ModelConfigRequest(protocol_req_tx)))
            }
            _ = self.update_stats_interval.tick() => {
                on_update_stats(self.router.endpoint(), &mut self.state).await?;
                Ok(None)
            }
            else => { Ok(None) }
        }
    }

    fn on_download_update(
        &mut self,
        update: DownloadUpdate,
    ) -> Option<NetworkEvent<BroadcastMessage, Download>> {
        self.state.bandwidth_tracker.add_event(
            update.blob_ticket.node_addr().node_id,
            update.downloaded_size_delta,
        );

        let hash = update.blob_ticket.hash();

        if update.all_done {
            self.state.download_progesses.remove(&hash);

            let blobs = self.blobs.client().clone();
            let (send, recv) = oneshot::channel();
            trace!(name: "blob_download_read_start", hash = hash.fmt_short());
            tokio::spawn(async move {
                let blob_bytes = match blobs.read_to_bytes(hash).await {
                    Ok(b) => b,
                    Err(err) => {
                        error!("Failed to read bytes: {err:#}");
                        return;
                    }
                };
                let size = blob_bytes.len();
                let res = send.send(blob_bytes);
                debug!(name: "blob_download_finish", hash = hash.fmt_short(), "downloaded blob {}, {} bytes", hash.fmt_short(), size);
                if res.is_err() {
                    error!("Failed to send read bytes result.");
                }
            });

            self.download_manager
                .read(update.blob_ticket, update.tag, recv, update.download_type);
        } else {
            self.state.download_progesses.insert(hash, update);
        }
        None
    }

    pub async fn get_all_peers(&self) -> Vec<(NodeAddr, ConnectionType)> {
        std::iter::once((
            self.router
                .endpoint()
                .node_addr()
                .await
                .expect("node addr exists"),
            ConnectionType::None,
        ))
        .chain(self.router.endpoint().remote_info_iter().map(|i| {
            let c = i.conn_type.clone();
            (i.into(), c)
        }))
        .collect()
    }

    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    pub fn neighbors(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.gossip_rx.neighbors()
    }
}

pub async fn request_model(
    router: Arc<Router>,
    node_addr: NodeId,
    request_type: &ModelRequestType,
) -> Result<BlobTicket> {
    let conn = router
        .endpoint()
        .connect(node_addr, p2p_model_sharing::ALPN)
        .await?;

    // Open a bidirectional QUIC stream
    let (mut send, mut recv) = conn.open_bi().await?;

    send.write_all(&request_type.to_bytes()).await?;
    send.finish()?;

    // Receive parameter value blob ticket
    let parameter_blob_ticket_bytes = recv.read_to_end(16384).await?;
    let parameter_blob_ticket: Result<BlobTicket, SharableModelError> =
        postcard::from_bytes(&parameter_blob_ticket_bytes)?;
    parameter_blob_ticket.with_context(|| "Error parsing model parameter blob ticket".to_string())
}

fn parse_gossip_event<BroadcastMessage: Networkable>(
    event: Result<iroh_gossip::net::Event>,
    gossip: &GossipReceiver,
) -> Option<(PublicKey, BroadcastMessage)> {
    match event {
        Ok(iroh_gossip::net::Event::Gossip(GossipEvent::Received(msg))) => {
            let message_hash = hash_bytes(&msg.content);
            match SignedMessage::<BroadcastMessage>::verify_and_decode(&msg.content) {
                Ok(result) => {
                    debug!(
                        name: "gossip_rx",
                        message_hash = message_hash,
                        "received gossip message with hash {message_hash}: {:?}",
                        result
                    );
                    return Some(result);
                }
                Err(err) => {
                    warn!("Got a gossip message delivered from {}, but could not verify / decode it! {err}", msg.delivered_from);
                }
            }
        }
        Ok(iroh_gossip::net::Event::Gossip(GossipEvent::Joined(peers))) => {
            debug!(name: "gossip_init", peers = ?peers, "gossip initialized with peers {peers:?}");
        }
        Ok(iroh_gossip::net::Event::Gossip(GossipEvent::NeighborUp(node_id))) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_new_peer", node_id=%node_id, all_gossip_peers = ?peers, "gossip connected to new peer {node_id}, we now have {} peers", peers.len());
        }
        Ok(iroh_gossip::net::Event::Gossip(GossipEvent::NeighborDown(node_id))) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_lost_peer", node_id=%node_id, all_gossip_peers = ?peers, "gossip disconnected from peer {node_id}, we now have {} peers", peers.len());
        }
        Ok(iroh_gossip::net::Event::Lagged) => {
            error!(name: "gossip_lagged","Gossip lagged. We missed some events.")
        }
        Err(err) => {
            warn!("Error on gossip event RX: {err}");
        }
    }

    None
}

#[derive(Debug)]
pub enum NetworkEvent<BM, D>
where
    BM: Networkable,
    D: Networkable,
{
    MessageReceived((PublicKey, BM)),
    DownloadComplete(DownloadComplete<D>),
    DownloadFailed(DownloadFailed),
    ParameterRequest(
        String,
        oneshot::Sender<Result<BlobTicket, SharableModelError>>,
    ),
    ModelConfigRequest(oneshot::Sender<Result<BlobTicket, SharableModelError>>),
}

async fn on_update_stats(endpoint: &Endpoint, stats: &mut State) -> Result<()> {
    let ticket = {
        let me = endpoint.node_addr().await?;
        PeerList(vec![me])
    };

    stats.join_ticket = ticket;

    for (peer_id, conn_type, last_recvd) in endpoint
        .remote_info_iter()
        .filter_map(|i| i.last_received().map(|r| (i.node_id, i.conn_type, r)))
    {
        // after 2 minutes with no comms, assume a client is disconnected.
        if last_recvd.as_secs() < 120 {
            stats
                .last_seen
                .insert(peer_id, (conn_type, Instant::now().sub(last_recvd)));
        } else {
            stats.last_seen.remove(&peer_id);
        }
    }

    stats
        .bandwidth_history
        .push_back(stats.bandwidth_tracker.get_total_bandwidth());
    const BANDWIDTH_GRAPH_SIZE: usize = 60;
    if stats.bandwidth_history.len() > BANDWIDTH_GRAPH_SIZE {
        stats.bandwidth_history.pop_front();
    }

    Ok(())
}

/// Get the Psyche [`RelayMap`].
pub fn psyche_relay_map() -> RelayMap {
    RelayMap::from_nodes([
        psyche_use_relay_node(),
        psyche_usw_relay_node(),
        psyche_euc_relay_node(),
    ])
    .expect("default nodes invalid")
}

/// Get the Psyche [`RelayNode`] for US East.
pub fn psyche_use_relay_node() -> RelayNode {
    let url: Url = format!("https://{USE_RELAY_HOSTNAME}")
        .parse()
        .expect("default url");
    RelayNode {
        url: url.into(),
        stun_only: false,
        stun_port: DEFAULT_STUN_PORT,
        quic: Some(RelayQuicConfig::default()),
    }
}

/// Get the Psyche [`RelayNode`] for US West.
pub fn psyche_usw_relay_node() -> RelayNode {
    let url: Url = format!("https://{USW_RELAY_HOSTNAME}")
        .parse()
        .expect("default_url");
    RelayNode {
        url: url.into(),
        stun_only: false,
        stun_port: DEFAULT_STUN_PORT,
        quic: Some(RelayQuicConfig::default()),
    }
}

/// Get the Psyche [`RelayNode`] for Europe
pub fn psyche_euc_relay_node() -> RelayNode {
    let url: Url = format!("https://{EUC_RELAY_HOSTNAME}")
        .parse()
        .expect("default_url");
    RelayNode {
        url: url.into(),
        stun_only: false,
        stun_port: DEFAULT_STUN_PORT,
        quic: Some(RelayQuicConfig::default()),
    }
}

fn hash_bytes(bytes: &Bytes) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[allow(clippy::too_many_arguments)]
pub async fn param_request_task(
    model_request_type: ModelRequestType,
    router: Arc<Router>,
    model_blob_tickets: Arc<StdMutex<Vec<(BlobTicket, ModelRequestType)>>>,
    peer_cycle: Arc<Mutex<VecDeque<PublicKey>>>,
    errored_peers: Arc<StdMutex<HashMap<PublicKey, usize>>>,
    num_peers: usize,
    cancel_token: CancellationToken,
) {
    let max_errors_per_peer: usize = 2;
    loop {
        let peer_id = match peer_cycle.lock().await.pop_front() {
            Some(peer) => peer,
            None => {
                continue;
            }
        };

        debug!(type = ?&model_request_type, peer = %peer_id, "Requesting model");
        let result = request_model(router.clone(), peer_id, &model_request_type)
            .timeout(Duration::from_secs(MODEL_REQUEST_TIMEOUT_SECS))
            .await
            .map_err(|_| anyhow!("Didn't receive the model resource in time"))
            .and_then(|inner| inner);

        match result {
            Ok(parameter_blob_ticket) => {
                model_blob_tickets
                    .lock()
                    .unwrap()
                    .push((parameter_blob_ticket, model_request_type.clone()));
                peer_cycle.lock().await.push_back(peer_id);
                break;
            }
            Err(e) => {
                let mut peer_cycle_lock = peer_cycle.lock().await;
                warn!(
                    parameter = ?&model_request_type,
                    peer = %peer_id,
                    "Failed to get parameter: {e}"
                );
                let mut errored_peers_lock = errored_peers.lock().unwrap();
                *errored_peers_lock.entry(peer_id).or_insert(0) += 1;
                if *errored_peers_lock.get(&peer_id).unwrap_or(&0) <= max_errors_per_peer {
                    peer_cycle_lock.push_back(peer_id);
                } else {
                    warn!(
                        "Not asking peer: {peer_id} because it's failing to retrieve us the model"
                    );
                }
                let min_peers_error_count = *errored_peers_lock.values().min().unwrap_or(&1);
                if errored_peers_lock.len() == num_peers
                    && min_peers_error_count >= max_errors_per_peer
                {
                    cancel_token.cancel();
                    break;
                }
                continue;
            }
        }
    }
}
