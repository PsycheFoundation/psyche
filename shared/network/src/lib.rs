use allowlist::Allowlist;
use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use download_manager::{DownloadManager, DownloadManagerEvent, DownloadUpdate};
use futures_util::{StreamExt, TryFutureExt};
use iroh::{Watcher, endpoint::TransportConfig, protocol::Router};
use iroh_blobs::api::Tag;
use iroh_blobs::api::tags::DeleteOptions;
use iroh_blobs::{
    BlobsProtocol,
    api::downloader::Downloader,
    store::{
        fs::options::GcConfig,
        mem::{MemStore, Options as MemStoreOptions},
    },
};
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::Gossip,
    proto::{HyparviewConfig, PlumtreeConfig},
};
pub use p2p_model_sharing::{
    MODEL_REQUEST_TIMEOUT_SECS, ModelConfigSharingMessage, ParameterSharingMessage,
    PeerManagerHandle,
};
use psyche_metrics::{ClientMetrics, PeerConnection};
use router::{SupportedProtocols, spawn_router_with_allowlist};
use state::State;
use std::{
    fmt::Debug,
    hash::{DefaultHasher, Hash as _, Hasher},
    marker::PhantomData,
    net::{IpAddr, Ipv4Addr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    select,
    sync::{mpsc::UnboundedReceiver, oneshot},
    task::JoinError,
    time::timeout,
};
use tokio::{
    sync::mpsc,
    time::{Interval, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, debug_span, error, info, trace, warn};
use util::{fmt_relay_mode, gossip_topic};

pub use ed25519::Signature;
pub use iroh::{NodeAddr, NodeId, RelayMode, endpoint::ConnectionType};
pub use iroh_blobs::{BlobFormat, Hash, ticket::BlobTicket};

pub mod allowlist;
mod authenticable_identity;
mod download_manager;
mod latency_sorted;
mod local_discovery;
mod p2p_model_sharing;
pub mod router;
mod serde;
mod serializable_kind;
mod serializable_tensor;
mod serialized_distro;
mod signed_message;
mod state;
mod tcp;
mod tui;
mod util;

#[cfg(test)]
mod test;

pub use authenticable_identity::{AuthenticatableIdentity, FromSignedBytesError, raw_p2p_verify};
pub use download_manager::{
    DownloadComplete, DownloadFailed, DownloadRetryInfo, DownloadType, MAX_DOWNLOAD_RETRIES,
    RetriedDownloadsHandle, TransmittableDownload,
};
pub use iroh::{Endpoint, PublicKey, SecretKey};
use iroh_relay::{RelayMap, RelayNode, RelayQuicConfig};
pub use latency_sorted::LatencySorted;
pub use p2p_model_sharing::{
    ALPN, ModelRequestType, SharableModel, SharableModelError, TransmittableModelConfig,
};
pub use serde::Networkable;
pub use serialized_distro::{
    SerializeDistroResultError, SerializedDistroResult, TransmittableDistroResult,
    distro_results_from_reader, distro_results_to_bytes,
};
pub use signed_message::SignedMessage;
pub use tcp::{ClientNotification, TcpClient, TcpServer};
pub use tui::{NetworkTUIState, NetworkTui};
use url::Url;
pub use util::fmt_bytes;

use crate::p2p_model_sharing::ModelSharing;

const USE_RELAY_HOSTNAME: &str = "use1-1.relay.nousresearch.psyche.iroh.link";
const USW_RELAY_HOSTNAME: &str = "usw1-1.relay.nousresearch.psyche.iroh.link";

/// How should this node discover other nodes?
///
/// In almost all cases, you want "N0", for over-the-internet communication.
/// For running tests, you might want Local, since Iroh's relay nodes have a rate limit per-ip.
#[derive(Debug, Clone, Copy)]
pub enum DiscoveryMode {
    Local,
    N0,
}

#[derive(Debug, Clone)]
pub struct P2PNodeInfo {
    pub node_id: NodeId,
    pub path: ConnectionType,
    pub bandwidth: f64,
    pub latency: f64,
}

impl From<P2PNodeInfo> for PeerConnection {
    fn from(value: P2PNodeInfo) -> Self {
        Self {
            node_id: value.node_id.to_string(),
            connection_type: match value.path {
                ConnectionType::None => psyche_metrics::ConnectionType::None,
                ConnectionType::Direct(..) => psyche_metrics::ConnectionType::Direct,
                ConnectionType::Mixed(..) => psyche_metrics::ConnectionType::Mixed,
                ConnectionType::Relay(..) => psyche_metrics::ConnectionType::Relay,
            },
            latency: value.latency as f32,
        }
    }
}

pub struct NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    router: Arc<Router>,
    blobs_store: MemStore,
    downloader: Downloader,
    state: State,
    gossip_tx: GossipSender,
    gossip_rx: GossipReceiver,
    rx_model_parameter_req: UnboundedReceiver<ParameterSharingMessage>,
    rx_model_config_req: UnboundedReceiver<ModelConfigSharingMessage>,
    download_manager: DownloadManager<Download>,
    _broadcast_message: PhantomData<BroadcastMessage>,
    _download: PhantomData<Download>,
    update_stats_interval: Interval,
    metrics: Arc<ClientMetrics>,
    endpoint: Endpoint,
}

impl<B, D> Debug for NetworkConnection<B, D>
where
    B: Networkable,
    D: Networkable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConnection")
            .field("router", &self.router)
            .field("blobs_store", &self.blobs_store)
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
    pub async fn init<A: Allowlist + 'static + Send + std::marker::Sync>(
        run_id: &str,
        port: Option<u16>,
        interface: Option<String>,
        discovery_mode: DiscoveryMode,
        bootstrap_peers: Vec<NodeAddr>,
        secret_key: Option<SecretKey>,
        allowlist: A,
        metrics: Arc<ClientMetrics>,
    ) -> Result<Self> {
        let secret_key = match secret_key {
            None => SecretKey::generate(&mut rand::rng()),
            Some(key) => key,
        };

        let public_key = secret_key.public();

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

        let bootstrap_node_ids = bootstrap_peers.iter().map(|p| p.node_id).collect();

        let endpoint = {
            let mut transport_config = TransportConfig::default();
            transport_config
                .max_idle_timeout(Some(Duration::from_secs(10).try_into()?))
                .keep_alive_interval(Some(Duration::from_secs(1)));

            let relay_mode = RelayMode::Custom(psyche_relay_map());
            debug!("Using relay servers: {}", fmt_relay_mode(&relay_mode));

            let endpoint = Endpoint::builder()
                .secret_key(secret_key)
                .relay_mode(relay_mode)
                .transport_config(transport_config)
                .bind_addr_v4(SocketAddrV4::new(ipv4, port.unwrap_or(0)));

            let endpoint = match discovery_mode {
                DiscoveryMode::Local => {
                    endpoint.discovery(local_discovery::LocalTestDiscovery::new(public_key))
                }
                DiscoveryMode::N0 => endpoint.discovery_n0(),
            };

            endpoint.bind().await?
        };

        if matches!(discovery_mode, DiscoveryMode::N0) {
            endpoint.online().await;
        }

        let node_addr = endpoint.node_addr();

        info!("Our node id: {}", node_addr.node_id);

        trace!("creating blobs store...");
        let store = MemStore::new_with_opts(MemStoreOptions {
            gc_config: Some(GcConfig {
                interval: Duration::from_secs(10),
                add_protected: None,
            }),
        });
        let downloader = Downloader::new(&store, &endpoint);
        trace!("blobs store created!");

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
            .spawn(endpoint.clone());
        trace!("gossip created!");

        trace!("creating model parameter sharing...");
        let (tx_model_parameter_req, rx_model_parameter_req) = mpsc::unbounded_channel();
        let (tx_model_config_req, rx_model_config_req) = mpsc::unbounded_channel();
        let model_parameter_sharing =
            ModelSharing::new(tx_model_parameter_req, tx_model_config_req);
        trace!("model parameter sharing created!");

        trace!("creating router...");
        let blobs_protocol = BlobsProtocol::new(&store.clone(), None);
        let router = spawn_router_with_allowlist(
            allowlist.clone(),
            endpoint.clone(),
            SupportedProtocols::new(gossip.clone(), blobs_protocol, model_parameter_sharing),
        )?;
        trace!("router created!");

        let (gossip_tx, gossip_rx) = gossip
            .subscribe(gossip_topic(run_id), bootstrap_node_ids)
            .await?
            .split();
        info!("Connected!");

        // if this is not 1s, the bandwidth chart will be wrong.
        let update_stats_interval = interval(Duration::from_secs(1));

        Ok(Self {
            blobs_store: store.clone(),
            downloader,
            gossip_rx,
            gossip_tx,
            rx_model_parameter_req,
            rx_model_config_req,

            router,
            metrics,

            update_stats_interval,
            state: State::new(15),
            download_manager: DownloadManager::new(store)?,
            _broadcast_message: Default::default(),
            _download: Default::default(),
            endpoint,
        })
    }

    pub async fn shutdown(&self) -> Result<(), JoinError> {
        self.router.shutdown().await
    }

    pub fn node_id(&self) -> NodeId {
        self.router.endpoint().node_id()
    }

    pub fn is_allowlisted<A: Allowlist>(node_id: &NodeId, allowlist: &A) -> bool {
        allowlist.allowed(*node_id)
    }

    /// Don't call this often / with many peers!
    /// It can force disconnection of other gossip peers if we have too many.
    pub fn add_peers(&self, peers: Vec<NodeId>) {
        let peer_list = peers
            .iter()
            .map(|n| n.fmt_short().to_string())
            .collect::<Vec<_>>()
            .join(",");
        debug!(name: "gossip_join_peers", peers=peer_list);
        let gossip_tx = self.gossip_tx.clone();
        let node_id = self.router.endpoint().node_id();
        tokio::task::spawn(
            async move {
                if let Err(err) = gossip_tx
                    .join_peers(peers.into_iter().filter(|p| p != &node_id).collect())
                    .await
                {
                    error!("Failed to join gossip peers: {err:#}")
                }
            }
            .instrument(debug_span!("gossip_join_peers", peers = peer_list)),
        );
    }

    pub fn broadcast(&self, message: &BroadcastMessage) -> Result<()> {
        let gossip_tx = self.gossip_tx.clone();
        let encoded_message =
            SignedMessage::sign_and_encode(self.router.endpoint().secret_key(), message)?;
        let message_hash = hash_bytes(&encoded_message);
        debug!(
            name: "gossip_broadcast",
            message_hash = message_hash,
            "broadcasted gossip message with hash {message_hash}: {:?}",
            message
        );
        tokio::spawn(async move { gossip_tx.broadcast(encoded_message).await });
        Ok(())
    }

    pub fn start_download(&mut self, ticket: BlobTicket, tag: Tag, download_type: DownloadType) {
        let provider_node_id = ticket.node_addr().clone();
        let ticket_hash = ticket.hash();
        let additional_peers_to_try = match download_type.clone() {
            DownloadType::DistroResult(peers) => peers,
            DownloadType::ModelSharing(_) => {
                vec![]
            }
        };
        debug!(name: "blob_download_start", hash = %ticket_hash.fmt_short(), "started downloading blob {}", ticket_hash);

        let latency_sorted = LatencySorted::new(
            std::iter::once(provider_node_id.node_id)
                .chain(additional_peers_to_try.iter().cloned())
                .collect(),
            self.endpoint.clone(),
        );
        let progress = self.downloader.download(ticket_hash, latency_sorted);
        let blob_store_clone = self.blobs_store.clone();
        self.download_manager
            .start_download(progress, ticket, tag.clone(), download_type);
        tokio::spawn(async move {
            let _ = blob_store_clone.tags().set(tag, ticket_hash).await;
        });
    }

    pub async fn add_downloadable(&mut self, data: Download, tag: Tag) -> Result<BlobTicket> {
        let blob_data = postcard::to_allocvec(&data)?;
        let blob_res = self
            .blobs_store
            .blobs()
            .add_bytes(blob_data.clone())
            .with_named_tag(tag)
            .await?;
        let addr = self.router.endpoint().node_addr();
        let blob_ticket = BlobTicket::new(addr, blob_res.hash, blob_res.format);
        debug!(
            name: "blob_upload",
            hash = %blob_res.hash.fmt_short(),
            size = blob_data.len(),
            "blob added for upload with hash {:?} with size {:?}",
            blob_res.hash.fmt_short(),
            blob_data.len()
        );

        Ok(blob_ticket)
    }

    /// Removes all the tags from the store that are lower than the target tag.
    /// Also removes all the tags used for the parameter sharing since this will run only in the Train state
    pub async fn remove_staled_tags(&mut self, target_tag: u32) -> anyhow::Result<()> {
        let delete_opts = DeleteOptions {
            from: None,
            to: Some(target_tag.to_string().into()),
        };
        let store = self.blobs_store.as_ref().clone();
        let model_tags_deleted = store.tags().delete_prefix("model-").await?;
        let distro_results_deleted = store.tags().delete_with_opts(delete_opts).await?;
        debug!(
            "Untagged {} blobs",
            model_tags_deleted + distro_results_deleted
        );
        Ok(())
    }

    pub async fn node_addr(&self) -> NodeAddr {
        self.router.endpoint().node_addr()
    }

    pub fn remote_infos(&self) -> Vec<P2PNodeInfo> {
        std::iter::once(P2PNodeInfo {
            node_id: self.endpoint.node_id(),
            bandwidth: 0.0,
            path: ConnectionType::None,
            latency: 0.0,
        })
        .chain(self.endpoint.connections().into_iter().map(|node_id| {
            let bandwidth = self
                .state
                .bandwidth_tracker
                .get_bandwidth_by_node(&node_id)
                .unwrap_or_default();
            P2PNodeInfo {
                node_id,
                path: self
                    .endpoint
                    .conn_type(node_id)
                    .map(|mut c| c.get())
                    .unwrap_or(ConnectionType::None),
                bandwidth,
                latency: self
                    .endpoint
                    .latency(node_id)
                    .unwrap_or(Duration::MAX)
                    .as_secs_f64(),
            }
        }))
        .collect()
    }

    pub async fn poll_next(&mut self) -> Result<Option<NetworkEvent<BroadcastMessage, Download>>> {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = self.gossip_rx.next() => {
                match parse_gossip_event(event.map_err(|ee| ee.into()), &self.gossip_rx, &self.metrics) {
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
                        self.metrics.update_download_progress(update.downloaded_size_delta);
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
                on_update_stats(&self.endpoint, self.remote_infos(), &mut self.state).await?;
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

            // let blobs = self.blobs_store.blobs().clone();
            // let (send, recv) = oneshot::channel();
            // trace!(name: "blob_download_read_start", hash = %hash.fmt_short());
            // tokio::spawn(async move {
            //     let mut buf = Vec::new();
            //     if let Err(err) = blobs.reader(hash).read_to_end(&mut buf).await {
            //         error!("Failed to read bytes: {err:#}");
            //         return;
            //     }
            //     let size = buf.len();
            //     let res = send.send(Bytes::from(buf));
            //     debug!(name: "blob_download_finish", hash = %hash.fmt_short(), "downloaded blob {:?}, {} bytes", hash.fmt_short(), size);
            //     if res.is_err() {
            //         error!("Failed to send read bytes result.");
            //     }
            // });

            self.download_manager
                .read(update.blob_ticket, update.tag, update.download_type);
        } else {
            self.state.download_progesses.insert(hash, update);
        }
        None
    }

    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    pub fn neighbors(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.gossip_rx.neighbors()
    }
}

pub async fn request_model_blob_ticket(
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
    let parameter_blob_ticket: Result<Result<BlobTicket, SharableModelError>, postcard::Error> =
        postcard::from_bytes(&parameter_blob_ticket_bytes);
    let result = parameter_blob_ticket
        .with_context(|| "Error parsing model parameter blob ticket".to_string())?;

    result.map_err(|e| anyhow!("Error received from peer: {e}"))
}

fn parse_gossip_event<BroadcastMessage: Networkable>(
    event: Result<iroh_gossip::api::Event>,
    gossip: &GossipReceiver,
    metrics: &ClientMetrics,
) -> Option<(PublicKey, BroadcastMessage)> {
    match event {
        Ok(iroh_gossip::api::Event::Received(msg)) => {
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
                    warn!(
                        "Got a gossip message delivered from {}, but could not verify / decode it! {err}",
                        msg.delivered_from
                    );
                }
            }
        }
        Ok(iroh_gossip::api::Event::NeighborUp(node_id)) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_new_peer", node_id=%node_id, all_gossip_peers = ?peers, "gossip connected to new peer {node_id}, we now have {} peers", peers.len());
            metrics.update_p2p_gossip_neighbors(&peers);
        }
        Ok(iroh_gossip::api::Event::NeighborDown(node_id)) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_lost_peer", node_id=%node_id, all_gossip_peers = ?peers, "gossip disconnected from peer {node_id}, we now have {} peers", peers.len());
            metrics.update_p2p_gossip_neighbors(&peers);
        }
        Ok(iroh_gossip::api::Event::Lagged) => {
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

async fn on_update_stats(
    endpoint: &Endpoint,
    remote_infos: Vec<P2PNodeInfo>,
    stats: &mut State,
) -> Result<()> {
    stats.node_id = Some(endpoint.node_id());

    stats.node_connections = remote_infos;

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
    RelayMap::from_iter([psyche_use_relay_node(), psyche_usw_relay_node()])
}

/// Get the Psyche [`RelayNode`] for US East.
pub fn psyche_use_relay_node() -> RelayNode {
    let url: Url = format!("https://{USE_RELAY_HOSTNAME}")
        .parse()
        .expect("default url");
    RelayNode {
        url: url.into(),
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
        quic: Some(RelayQuicConfig::default()),
    }
}

fn hash_bytes(bytes: &Bytes) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

// Simplified param_request_task
pub async fn blob_ticket_param_request_task(
    model_request_type: ModelRequestType,
    router: Arc<Router>,
    model_blob_tickets: Arc<std::sync::Mutex<Vec<(BlobTicket, ModelRequestType)>>>,
    peer_manager: Arc<PeerManagerHandle>,
    cancellation_token: CancellationToken,
) {
    let max_attempts = 500u16;
    let mut attempts = 0u16;

    while attempts < max_attempts {
        let Some(peer_id) = peer_manager.get_next_peer().await else {
            // No peers available, wait a bit and check again
            tokio::time::sleep(Duration::from_millis(500)).await;
            attempts += 1;
            continue;
        };

        info!(type = ?&model_request_type, peer = %peer_id, "Requesting model");
        let result = timeout(
            Duration::from_secs(MODEL_REQUEST_TIMEOUT_SECS),
            request_model_blob_ticket(router.clone(), peer_id, &model_request_type),
        )
        .map_err(|e| anyhow!("{e}"))
        .await;

        match result {
            Ok(Ok(blob_ticket)) => {
                model_blob_tickets
                    .lock()
                    .unwrap()
                    .push((blob_ticket, model_request_type));

                peer_manager.report_success(peer_id);
                return;
            }
            Ok(Err(e)) | Err(e) => {
                // Failed - report error and potentially try next peer
                peer_manager.report_blob_ticket_request_error(peer_id, None);

                warn!("Request failed for peer {peer_id}: {e}. Trying next peer");
                attempts += 1;

                // Small delay before retry
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    error!("No peers available to give us a model parameter after {max_attempts} attempts");
    cancellation_token.cancel();
}
