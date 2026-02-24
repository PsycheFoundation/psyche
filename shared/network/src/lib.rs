use allowlist::Allowlist;
use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use download::{DownloadManager, DownloadManagerEvent, DownloadUpdate};
use futures_util::{StreamExt, TryFutureExt};
use iroh::{EndpointAddr, RelayConfig};
use iroh::{endpoint::QuicTransportConfig, protocol::Router};
use iroh_blobs::api::Tag;
use iroh_blobs::store::GcConfig;
use iroh_blobs::{
    BlobsProtocol,
    api::downloader::Downloader,
    store::mem::{MemStore, Options as MemStoreOptions},
};
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::Gossip,
    proto::{HyparviewConfig, PlumtreeConfig},
};
use iroh_n0des::ApiSecret;
pub use p2p_model_sharing::{
    MODEL_REQUEST_TIMEOUT_SECS, ModelConfigSharingMessage, ParameterSharingMessage,
    PeerManagerHandle,
};
use psyche_metrics::{ClientMetrics, PeerConnection};
use router::{SupportedProtocols, spawn_router};
use state::State;
use std::str::FromStr;
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

pub use ed25519_dalek::Signature;
pub use iroh::RelayMode;
pub use iroh_blobs::{BlobFormat, Hash, ticket::BlobTicket};

pub mod allowlist;
mod authenticable_identity;
mod connection_monitor;
mod download;
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
pub use connection_monitor::{ConnectionData, ConnectionMonitor, PeerBandwidth};
pub use download::{
    DownloadComplete, DownloadFailed, DownloadSchedulerHandle, DownloadType, ReadyRetry,
    RetryConfig, RetryQueueResult, TransmittableDownload,
};
pub use iroh::protocol::ProtocolHandler;
pub use iroh::{Endpoint, EndpointId, PublicKey, SecretKey};
use iroh_relay::{RelayMap, RelayQuicConfig};
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

use crate::allowlist::AllowlistHook;
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

impl FromStr for DiscoveryMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(DiscoveryMode::Local),
            "n0" => Ok(DiscoveryMode::N0),
            _ => Err(format!(
                "Invalid discovery mode: '{}'. Expected 'local' or 'n0'",
                s
            )),
        }
    }
}

/// What relays should we connect to?
#[derive(Debug, Clone, Copy)]
pub enum RelayKind {
    /// No relays (for local tests)
    Disabled,
    /// Psyche-specific relays
    Psyche,
    /// N0 default relays
    N0,
}

impl FromStr for RelayKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disabled" => Ok(RelayKind::Disabled),
            "psyche" => Ok(RelayKind::Psyche),
            "n0" => Ok(RelayKind::N0),
            _ => Err(format!(
                "Invalid relay kind: '{}'. Expected 'psyche' or 'n0'",
                s
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct P2PEndpointInfo {
    pub id: EndpointId,
    pub selected_path: Option<psyche_metrics::SelectedPath>,
    pub bandwidth: f64,
    pub latency: f64,
}

impl From<P2PEndpointInfo> for PeerConnection {
    fn from(value: P2PEndpointInfo) -> Self {
        Self {
            endpoint_id: value.id.to_string(),
            selected_path: value.selected_path,
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
    connection_monitor: ConnectionMonitor,
    _iroh_metrics: Option<iroh_n0des::Client>,
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
        relay_kind: RelayKind,
        bootstrap_peers: Vec<EndpointAddr>,
        secret_key: Option<SecretKey>,
        allowlist: A,
        metrics: Arc<ClientMetrics>,
        cancel: Option<CancellationToken>,
    ) -> Result<Self> {
        Self::init_internal::<A, iroh_gossip::net::Gossip>(
            run_id,
            port,
            interface,
            discovery_mode,
            relay_kind,
            bootstrap_peers,
            secret_key,
            allowlist,
            metrics,
            cancel,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn init_with_custom_protocol<
        A: Allowlist + 'static + Send + std::marker::Sync,
        P: ProtocolHandler + Clone,
    >(
        run_id: &str,
        port: Option<u16>,
        interface: Option<String>,
        discovery_mode: DiscoveryMode,
        relay_kind: RelayKind,
        bootstrap_peers: Vec<EndpointAddr>,
        secret_key: Option<SecretKey>,
        allowlist: A,
        metrics: Arc<ClientMetrics>,
        cancel: Option<CancellationToken>,
        additional_protocol: (&'static [u8], P),
    ) -> Result<Self> {
        Self::init_internal(
            run_id,
            port,
            interface,
            discovery_mode,
            relay_kind,
            bootstrap_peers,
            secret_key,
            allowlist,
            metrics,
            cancel,
            Some(additional_protocol),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn init_internal<
        A: Allowlist + 'static + Send + std::marker::Sync,
        P: ProtocolHandler + Clone,
    >(
        run_id: &str,
        port: Option<u16>,
        interface: Option<String>,
        discovery_mode: DiscoveryMode,
        relay_kind: RelayKind,
        bootstrap_peers: Vec<EndpointAddr>,
        secret_key: Option<SecretKey>,
        allowlist: A,
        metrics: Arc<ClientMetrics>,
        cancel: Option<CancellationToken>,
        additional_protocol: Option<(&'static [u8], P)>,
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

        let bootstrap_endpoint_ids = bootstrap_peers.iter().map(|p| p.id).collect();

        let connection_monitor = ConnectionMonitor::default();

        let allowlist_hook = AllowlistHook::new(allowlist.clone());

        let endpoint = {
            let transport_config = QuicTransportConfig::builder()
                .max_idle_timeout(Some(Duration::from_secs(10).try_into()?))
                .keep_alive_interval(Duration::from_secs(1))
                .set_max_remote_nat_traversal_addresses(50)
                .build();

            let relay_mode = match relay_kind {
                RelayKind::Disabled => RelayMode::Disabled,
                RelayKind::N0 => RelayMode::Default,
                RelayKind::Psyche => RelayMode::Custom(psyche_relay_map()),
            };
            debug!("Using relay servers: {}", fmt_relay_mode(&relay_mode));

            let endpoint = Endpoint::builder()
                .secret_key(secret_key)
                .relay_mode(relay_mode)
                .transport_config(transport_config)
                .bind_addr(SocketAddrV4::new(ipv4, port.unwrap_or(0)))?
                .clear_address_lookup()
                .hooks(allowlist_hook.clone())
                .hooks(connection_monitor.clone());

            let endpoint = match discovery_mode {
                DiscoveryMode::Local => {
                    endpoint.address_lookup(local_discovery::LocalTestDiscovery::new(public_key))
                }
                DiscoveryMode::N0 => {
                    let dns = iroh::address_lookup::dns::DnsAddressLookup::n0_dns().build();
                    let pkarr = iroh::address_lookup::pkarr::PkarrPublisher::n0_dns();

                    endpoint.address_lookup(dns).address_lookup(pkarr)
                }
            };

            endpoint.bind().await?
        };

        // Wait until the endpoint is online if using N0 discovery
        // The cancel token allows to exit the client via Ctrl+C instead of hanging
        if matches!(discovery_mode, DiscoveryMode::N0) {
            if let Some(cancel_token) = &cancel {
                select! {
                    _ = endpoint.online() => {},
                    _ = cancel_token.cancelled() => {
                        return Err(anyhow!("Cancelled by user"));
                    }
                }
            } else {
                endpoint.online().await;
            }
        }

        let endpoint_addr = endpoint.addr();

        info!("Our endpoint ID: {}", endpoint_addr.id);

        let iroh_metrics = {
            let builder = iroh_n0des::Client::builder(&endpoint);
            let allowlist = allowlist.clone();
            (async move {
                let client = builder.api_secret_from_env()?.build().await?;
                const API_SECRET_ENV_VAR_NAME: &str = "N0DES_API_SECRET";

                match std::env::var(API_SECRET_ENV_VAR_NAME) {
                    Ok(ticket_string) => {
                        let ticket = ApiSecret::from_str(&ticket_string)
                            .context(format!("invalid {API_SECRET_ENV_VAR_NAME}"))?;
                        let endpoint_id = ticket.remote.id;
                        allowlist.force_allow(endpoint_id);
                    }
                    Err(e) => unreachable!("{e:?}"),
                }
                Ok(client)
            })
            .await as anyhow::Result<iroh_n0des::Client>
        }
        .map_or_else(
            |e| {
                info!("Iroh metrics not enabled: {e:?}");
                None
            },
            Some,
        );

        trace!("creating blobs store...");

        let gc_interval: u64 = std::env::var("BLOBS_GC_INTERVAL_MILLIS")
            .ok()
            .and_then(|gc_interval_str| gc_interval_str.parse().ok())
            .unwrap_or(10000);

        let store = MemStore::new_with_opts(MemStoreOptions {
            gc_config: Some(GcConfig {
                interval: Duration::from_millis(gc_interval),
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
        let router = spawn_router(
            endpoint.clone(),
            SupportedProtocols::new(gossip.clone(), blobs_protocol, model_parameter_sharing),
            additional_protocol,
        )?;
        trace!("router created!");

        let (gossip_tx, gossip_rx) = gossip
            .subscribe(gossip_topic(run_id), bootstrap_endpoint_ids)
            .await?
            .split();
        info!("Connected!");

        // if this is not 1s, the bandwidth chart will be wrong.
        let update_stats_interval = interval(Duration::from_secs(1));

        Ok(Self {
            blobs_store: store,
            downloader,
            gossip_rx,
            gossip_tx,
            rx_model_parameter_req,
            rx_model_config_req,

            router,
            metrics,

            update_stats_interval,
            state: State::new(15),
            download_manager: DownloadManager::new()?,
            _broadcast_message: Default::default(),
            _download: Default::default(),
            endpoint,
            connection_monitor,
            _iroh_metrics: iroh_metrics,
        })
    }

    pub async fn shutdown(&self) -> Result<(), JoinError> {
        self.router.shutdown().await
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.router.endpoint().id()
    }

    pub fn is_allowlisted<A: Allowlist>(endpoint_id: &EndpointId, allowlist: &A) -> bool {
        allowlist.allowed(*endpoint_id)
    }

    /// Don't call this often / with many peers!
    /// It can force disconnection of other gossip peers if we have too many.
    pub fn add_peers(&self, peers: Vec<EndpointId>) {
        let peer_list = peers
            .iter()
            .map(|n| n.fmt_short().to_string())
            .collect::<Vec<_>>()
            .join(",");
        debug!(name: "gossip_join_peers", peers=peer_list);
        let gossip_tx = self.gossip_tx.clone();
        let endpoint_id = self.router.endpoint().id();
        tokio::task::spawn(
            async move {
                if let Err(err) = gossip_tx
                    .join_peers(peers.into_iter().filter(|p| p != &endpoint_id).collect())
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
        let provider_endpoint_id = ticket.addr().clone();
        let ticket_hash = ticket.hash();
        let additional_peers_to_try = match download_type.clone() {
            DownloadType::DistroResult(peers) => peers,
            DownloadType::ModelSharing(_) => {
                vec![]
            }
        };
        let (tx, rx) = mpsc::unbounded_channel();
        // We share the tag with the download manager to keep track of the download progress on this blob but we actually set the tag here
        self.download_manager
            .add(ticket, tag.clone(), rx, download_type.clone());
        debug!(name: "blob_download_start", hash = %ticket_hash.fmt_short(), "started downloading blob {}", ticket_hash);

        let latency_sorted = LatencySorted::new(
            std::iter::once(provider_endpoint_id.id)
                .chain(additional_peers_to_try.iter().cloned())
                .collect(),
            self.connection_monitor.clone(),
        );
        let download = self.downloader.download(ticket_hash, latency_sorted);
        let blob_store_clone = self.blobs_store.clone();
        tokio::spawn(async move {
            let _ = blob_store_clone.tags().set(tag, ticket_hash).await;
            let progress = download.stream().await;

            match progress {
                Ok(mut progress) => {
                    let result = tokio::time::timeout(Duration::from_secs(300), async {
                        while let Some(val) = progress.next().await {
                            if let Err(err) = tx.send(Ok(val)) {
                                panic!("Failed to send download progress: {err:?} {:?}", err.0);
                            }
                        }
                    })
                    .await;

                    if result.is_err() {
                        warn!(
                            "Download of blob {} timed out after 5 minutes",
                            ticket_hash.fmt_short()
                        );
                        let _ = tx.send(Err(anyhow!("Download timed out after 5 minutes")));
                    }
                }
                Err(e) => panic!("Failed to start download: {e}"),
            }
        });
    }

    pub async fn add_downloadable(
        &mut self,
        data: Download,
        tag: Tag,
    ) -> Result<(BlobTicket, usize)> {
        let blob_data = postcard::to_allocvec(&data)?;
        let blob_res = self
            .blobs_store
            .blobs()
            .add_bytes(blob_data.clone())
            .with_named_tag(tag)
            .await?;
        let addr = self.router.endpoint().addr();
        let blob_ticket = BlobTicket::new(addr, blob_res.hash, blob_res.format);
        debug!(
            name: "blob_upload",
            hash = %blob_res.hash.fmt_short(),
            size = blob_data.len(),
            "blob added for upload with hash {:?} with size {:?}",
            blob_res.hash.fmt_short(),
            blob_data.len()
        );

        Ok((blob_ticket, blob_data.len()))
    }

    /// Removes all the tags from the store that are lower than the target tag.
    /// Also removes all the tags used for the parameter sharing since this will run only in the Train state
    pub async fn remove_staled_tags(
        &mut self,
        target_distro_result_step: u32,
    ) -> anyhow::Result<()> {
        let store = self.blobs_store.as_ref().clone();
        let model_tags_deleted = store.tags().delete_prefix("model-").await?;
        let mut distro_results_deleted = 0;
        let mut tags = store.tags().list().await?;

        while let Some(tag) = tags.next().await {
            let Ok(tag) = tag else {
                warn!("Error while getting tag: {tag:?}. This may lead to a memory leak");
                continue;
            };

            let Ok(tag_name) = String::from_utf8(tag.name.0.to_vec()) else {
                warn!(
                    "Error while decoding tag name to string: {tag:?}. This may lead to a memory leak"
                );
                continue;
            };

            // Since tags related to model parameter sharing have been already deleted, it is assumed that
            // all remaining tags are related to Distro result blobs
            let tag_name_splitted: Vec<&str> = tag_name.split("_").collect();
            let Some(tag_name_distro_result_step) = tag_name_splitted.get(1) else {
                warn!("Step not present in tag name: {tag_name}. This may lead to a memory leak");
                continue;
            };
            let Ok(distro_result_step) = tag_name_distro_result_step.parse::<u32>() else {
                warn!(
                    "Distro result step could not be parsed: {tag_name_distro_result_step}. This may lead to a memory leak"
                );
                continue;
            };

            if distro_result_step < target_distro_result_step {
                let tag_delete_res = store.tags().delete(&tag_name).await;
                if tag_delete_res.is_ok() {
                    distro_results_deleted += 1;
                } else {
                    warn!(
                        "There was an error while trying to delete tag {tag_name}: {tag_delete_res:?}"
                    );
                }
            }
        }

        debug!(
            "Untagged {} blobs",
            model_tags_deleted + distro_results_deleted
        );
        Ok(())
    }

    pub async fn endpoint_addr(&self) -> EndpointAddr {
        self.router.endpoint().addr()
    }

    pub fn remote_infos(&self) -> Vec<P2PEndpointInfo> {
        // start with our own endpoint
        let mut infos = vec![P2PEndpointInfo {
            id: self.endpoint.id(),
            bandwidth: 0.0,
            selected_path: None,
            latency: 0.0,
        }];

        // add all tracked connections
        for conn_data in self.connection_monitor.get_all_connections() {
            let latency = conn_data
                .latency()
                .map(|d| d.as_secs_f64())
                .unwrap_or(f64::MAX);
            let bandwidth = match conn_data.bandwidth {
                PeerBandwidth::NotMeasured => 0.0,
                PeerBandwidth::Measured(bw) => bw,
            };
            infos.push(P2PEndpointInfo {
                id: conn_data.endpoint_id,
                selected_path: conn_data.selected_path,
                bandwidth,
                latency,
            });
        }

        infos
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
                        let peer_id = result.blob_ticket.addr().id;
                        self.connection_monitor.update_peer_bandwidth(&peer_id, PeerBandwidth::Measured(0.0));
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
        let peer_id = update.blob_ticket.addr().id;
        self.state
            .bandwidth_tracker
            .add_event(peer_id, update.downloaded_size_delta);

        let peer_bw = self.state.bandwidth_tracker.get_peer_bandwidth(&peer_id);
        self.connection_monitor
            .update_peer_bandwidth(&peer_id, peer_bw);

        let hash = update.blob_ticket.hash();

        if update.all_done {
            self.state.download_progesses.remove(&hash);

            let blobs = self.blobs_store.blobs().clone();
            let (send, recv) = oneshot::channel();
            trace!(name: "blob_download_read_start", hash = %hash.fmt_short());
            tokio::spawn(async move {
                let mut buf = Vec::new();
                if let Err(err) = blobs.reader(hash).read_to_end(&mut buf).await {
                    error!("Failed to read bytes: {err:#}");
                    return;
                }
                let size = buf.len();
                let res = send.send(Bytes::from(buf));
                debug!(name: "blob_download_finish", hash = %hash.fmt_short(), "downloaded blob {:?}, {} bytes", hash.fmt_short(), size);
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
    pub fn connection_monitor(&self) -> ConnectionMonitor {
        self.connection_monitor.clone()
    }

    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    pub fn neighbors(&self) -> impl Iterator<Item = EndpointId> + '_ {
        self.gossip_rx.neighbors()
    }
}

pub async fn request_model_blob_ticket(
    router: Arc<Router>,
    endpoint_addr: EndpointId,
    request_type: &ModelRequestType,
) -> Result<BlobTicket> {
    let conn = router
        .endpoint()
        .connect(endpoint_addr, p2p_model_sharing::ALPN)
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
        Ok(iroh_gossip::api::Event::NeighborUp(endpoint_id)) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_new_peer", endpoint_id=%endpoint_id, all_gossip_peers = ?peers, "gossip connected to new peer {endpoint_id}, we now have {} peers", peers.len());
            metrics.update_p2p_gossip_neighbors(&peers);
        }
        Ok(iroh_gossip::api::Event::NeighborDown(endpoint_id)) => {
            let peers: Vec<_> = gossip.neighbors().collect();
            debug!(name: "gossip_lost_peer", endpoint_id=%endpoint_id, all_gossip_peers = ?peers, "gossip disconnected from peer {endpoint_id}, we now have {} peers", peers.len());
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
    remote_infos: Vec<P2PEndpointInfo>,
    stats: &mut State,
) -> Result<()> {
    stats.endpoint_id = Some(endpoint.id());

    stats.connection_info = remote_infos;

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

/// Get the Psyche [`RelayConfig`] for US East.
pub fn psyche_use_relay_node() -> RelayConfig {
    let url: Url = format!("https://{USE_RELAY_HOSTNAME}")
        .parse()
        .expect("default url");
    RelayConfig {
        url: url.into(),
        quic: Some(RelayQuicConfig::default()),
    }
}

/// Get the Psyche [`RelayConfig`] for US West.
pub fn psyche_usw_relay_node() -> RelayConfig {
    let url: Url = format!("https://{USW_RELAY_HOSTNAME}")
        .parse()
        .expect("default_url");
    RelayConfig {
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
    peer_manager: Arc<PeerManagerHandle>,
    cancellation_token: CancellationToken,
) -> Result<(BlobTicket, ModelRequestType)> {
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
                peer_manager.report_success(peer_id);
                return Ok((blob_ticket, model_request_type));
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
    Err(anyhow!(
        "Failed to get model parameter blob ticket after {max_attempts} attempts"
    ))
}
