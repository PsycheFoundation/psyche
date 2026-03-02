use anyhow::Result;
use clap::Parser;
use iroh_blobs::BlobFormat;
use iroh_blobs::Hash;
use iroh_blobs::api::Tag;
use iroh_blobs::ticket::BlobTicket;
use iroh_fake_store::FakeStore;
use postcard;
use psyche_metrics::ClientMetrics;
use psyche_network::{
    ConnectionMonitor, DiscoveryMode, DownloadType, EndpointId, ModelRequestType,
    NetworkConnection, NetworkEvent, PeerBandwidth, PeerManagerHandle, PublicKey, RelayKind,
    TransmittableDownload, TransmittableModelConfig, allowlist, blob_ticket_param_request_task,
};
use psyche_tui::LogOutput;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::select;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "model_sharing_test")]
#[command(about = "Test harness for P2P model sharing flow")]
struct CliArgs {
    /// Number of sharer peers
    #[clap(long, default_value_t = 1)]
    num_sharers: usize,

    /// Number of downloader peers
    #[clap(long, default_value_t = 2)]
    num_downloaders: usize,

    /// Number of model parameters to share
    #[clap(long, default_value_t = 300)]
    num_parameters: usize,

    /// Size of each parameter in MB
    #[clap(long, default_value_t = 1000)]
    parameter_size_mb: usize,

    /// Maximum concurrent downloads per downloader
    #[clap(long, default_value_t = 4)]
    max_concurrent_downloads: usize,

    /// Discovery mode: "local" or "n0"
    #[clap(long, default_value = "n0")]
    discovery_mode: String,

    /// Relay kind: "disabled", "psyche", or "n0"
    #[clap(long, default_value = "n0")]
    relay_kind: String,

    /// Number of sharers that should be slow (throttled).
    /// These will be the last N sharers created.
    #[clap(long, default_value_t = 0)]
    slow_sharers: usize,

    /// Bandwidth limit for slow sharers in KB/s (e.g., 100 = 100 KB/s)
    #[clap(long, default_value_t = 100)]
    slow_sharer_rate_kb: u64,
}

/// Gossip broadcast message type (unused but required by NetworkConnection generics)
#[derive(Debug, Serialize, Deserialize)]
enum TestMessage {
    Noop,
}

type NC = NetworkConnection<TestMessage, TransmittableDownload>;

fn generate_parameter_names(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("model.layers.{i}.weight"))
        .collect()
}

/// Spin up a NetworkConnection with the given discovery and relay modes
async fn create_peer(
    label: &str,
    discovery_mode: DiscoveryMode,
    relay_kind: RelayKind,
) -> Result<NC> {
    let metrics = Arc::new(ClientMetrics::new(None, None));
    let network = NC::init(
        "model-sharing-test",
        None,
        None,
        discovery_mode,
        relay_kind,
        vec![],
        None,
        allowlist::AllowAll,
        metrics,
        None,
    )
    .await?;

    info!(
        "{label} initialized with endpoint_id: {}",
        network.endpoint_id()
    );
    Ok(network)
}

/// Spin up a sharer NetworkConnection that uses FakeStore for blob serving.
/// This avoids holding large parameter data in memory.
async fn create_sharer_peer(
    label: &str,
    discovery_mode: DiscoveryMode,
    relay_kind: RelayKind,
    fake_store: &FakeStore,
) -> Result<NC> {
    let metrics = Arc::new(ClientMetrics::new(None, None));
    let store: iroh_blobs::api::Store = std::ops::Deref::deref(fake_store).clone();
    let network = NC::init_with_blobs_store(
        "model-sharing-test",
        None,
        None,
        discovery_mode,
        relay_kind,
        vec![],
        None,
        allowlist::AllowAll,
        metrics,
        None,
        store,
    )
    .await?;

    info!(
        "{label} initialized with endpoint_id: {} (FakeStore blobs)",
        network.endpoint_id()
    );
    Ok(network)
}

fn format_bandwidth(bw: &PeerBandwidth) -> String {
    match bw {
        PeerBandwidth::NotMeasured => "not measured".to_string(),
        PeerBandwidth::Measured(bytes_per_sec) => {
            format!("{:.2} MB/s", bytes_per_sec / (1024.0 * 1024.0))
        }
    }
}

fn print_peer_status(monitor: &ConnectionMonitor, context: &str) {
    let connections = monitor.get_all_connections();
    println!("\n  [Peer Status: {context}] ({} peers)", connections.len());
    for conn in &connections {
        let latency = conn
            .latency()
            .map(|d| format!("{d:?}"))
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "    {} | bw: {} | latency: {}",
            conn.endpoint_id,
            format_bandwidth(&conn.bandwidth),
            latency,
        );
    }
}

/// Run a sharer peer using FakeStore for blob data (zero memory for large params).
///
/// The NetworkConnection's BlobsProtocol is backed by FakeStore, so blobs are
/// generated on-the-fly without storing actual data in memory.
async fn run_sharer(
    mut network: NC,
    fake_store: FakeStore,
    param_names: Vec<String>,
    param_size_bytes: usize,
    cancel: CancellationToken,
) -> Result<()> {
    let endpoint_id = network.endpoint_id();
    info!(
        "Sharer {endpoint_id}: preparing {} parameters ({} MB each) using FakeStore",
        param_names.len(),
        param_size_bytes / (1024 * 1024)
    );

    // Get FakeStore blob hashes and create tickets pointing to our own endpoint
    let endpoint_addr = network.endpoint_addr().await;
    let fake_hashes = fake_store.blobs().list().hashes().await?;
    let mut param_tickets: HashMap<String, BlobTicket> = HashMap::new();
    for (name, hash) in param_names.iter().zip(fake_hashes.iter()) {
        let ticket = BlobTicket::new(endpoint_addr.clone(), *hash, BlobFormat::Raw);
        info!(
            "Sharer {endpoint_id}: parameter '{name}' -> hash {} (FakeStore)",
            hash.fmt_short()
        );
        param_tickets.insert(name.clone(), ticket);
    }

    // Add model config to FakeStore (BlobsProtocol serves from FakeStore, not MemStore)
    let config = TransmittableModelConfig::new(
        r#"{"model_type": "test", "num_layers": 10}"#.to_string(),
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":null,"post_processor":null,"decoder":null,"model":{"type":"BPE","dropout":null,"unk_token":null,"continuing_subword_prefix":null,"end_of_word_suffix":null,"fuse_unk":false,"byte_fallback":false,"ignore_merges":false,"vocab":{},"merges":[]}}"#.to_string(),
        param_names.clone(),
    );
    let config_transmittable = TransmittableDownload::ModelConfig(config);
    let config_bytes = postcard::to_allocvec(&config_transmittable)?;
    let config_size = config_bytes.len();
    let config_blob = fake_store
        .blobs()
        .add_bytes(config_bytes)
        .with_named_tag(Tag::from("model-config-share"))
        .await?;
    let config_ticket =
        BlobTicket::new(endpoint_addr.clone(), config_blob.hash, config_blob.format);
    info!(
        "Sharer {endpoint_id}: added model config ({config_size} bytes), hash: {}",
        config_ticket.hash()
    );

    // Event loop: handle parameter and config requests
    info!("Sharer {endpoint_id}: ready to serve requests");
    loop {
        select! {
            _ = cancel.cancelled() => {
                info!("Sharer {endpoint_id}: shutting down");
                break;
            }
            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::ParameterRequest(name, reply_tx))) => {
                        match param_tickets.get(&name) {
                            Some(ticket) => {
                                info!("Sharer {endpoint_id}: serving parameter '{name}' (FakeStore blob)");
                                let _ = reply_tx.send(Ok(ticket.clone()));
                            }
                            None => {
                                warn!("Sharer {endpoint_id}: unknown parameter '{name}'");
                                let _ = reply_tx.send(Err(
                                    psyche_network::SharableModelError::ParameterUnknown(name),
                                ));
                            }
                        }
                    }
                    Ok(Some(NetworkEvent::ModelConfigRequest(reply_tx))) => {
                        info!("Sharer {endpoint_id}: serving model config");
                        let _ = reply_tx.send(Ok(config_ticket.clone()));
                    }
                    Ok(Some(other)) => {
                        info!("Sharer {endpoint_id}: got event: {other:?}");
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Sharer {endpoint_id}: network error: {e:#}");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// A ticket obtained by a worker, sent to the main loop for downloading.
struct DownloadRequest {
    name: String,
    ticket: BlobTicket,
    peer_id: EndpointId,
    start_time: Instant,
    done_tx: oneshot::Sender<bool>,
}

/// Tracks an in-flight download in the main loop.
struct InFlightDownload {
    name: String,
    tag_name: String,
    peer_id: EndpointId,
    start_time: Instant,
    done_tx: oneshot::Sender<bool>,
}

/// Run a downloader peer using the full production download pipeline:
/// - Workers use `blob_ticket_param_request_task` (production peer selection + ticket request)
/// - Main loop calls `network.start_download()` (DownloadManager + iroh Downloader)
/// - Main loop calls `network.poll_next()` (BandwidthTracker → ConnectionMonitor integration)
/// - Tag cleanup after each download keeps memory bounded
async fn run_downloader(
    mut network: NC,
    sharer_ids: Vec<EndpointId>,
    expected_param_count: usize,
    _param_size_bytes: usize,
    max_concurrent: usize,
    cancel: CancellationToken,
) -> Result<DownloaderReport> {
    let endpoint_id = network.endpoint_id();
    info!(
        "Downloader {endpoint_id}: starting with {} sharer(s), max concurrent: {max_concurrent}",
        sharer_ids.len()
    );

    let connection_monitor = network.connection_monitor();
    let router = network.router();

    // Create PeerManager (mirrors client.rs)
    let peer_manager = Arc::new(PeerManagerHandle::new(
        3, // max_errors_per_peer
        cancel.clone(),
        connection_monitor.clone(),
    ));
    peer_manager.set_peers(sharer_ids.clone());

    print_peer_status(
        &connection_monitor,
        &format!("Downloader {endpoint_id} initial"),
    );

    let overall_start = Instant::now();
    let mut param_reports: Vec<ParamDownloadReport> = Vec::new();

    // Step 1: Request model config via production flow
    info!("Downloader {endpoint_id}: requesting model config...");
    let config_start = Instant::now();
    let (config_ticket, _) = blob_ticket_param_request_task(
        ModelRequestType::Config,
        router.clone(),
        peer_manager.clone(),
        cancel.clone(),
    )
    .await?;
    let config_request_time = config_start.elapsed();
    info!(
        "Downloader {endpoint_id}: got config ticket in {config_request_time:?}, starting download..."
    );

    network.start_download(
        config_ticket,
        iroh_blobs::api::Tag::from("model-config"),
        DownloadType::ModelSharing(ModelRequestType::Config),
    );

    // Wait for config download
    let param_names = loop {
        select! {
            _ = cancel.cancelled() => {
                return Err(anyhow::anyhow!("Cancelled while downloading config"));
            }
            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::DownloadComplete(result))) => {
                        match result.data {
                            TransmittableDownload::ModelConfig(config) => {
                                info!("Downloader {endpoint_id}: config downloaded with {} parameters in {:?}",
                                    config.parameter_names.len(), config_start.elapsed());
                                break config.parameter_names;
                            }
                            _ => {
                                warn!("Downloader {endpoint_id}: unexpected download type for config");
                            }
                        }
                    }
                    Ok(Some(NetworkEvent::DownloadFailed(f))) => {
                        error!("Downloader {endpoint_id}: config download failed: {}", f.error);
                        return Err(anyhow::anyhow!("Config download failed: {}", f.error));
                    }
                    Ok(_) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    };

    assert_eq!(
        param_names.len(),
        expected_param_count,
        "Config parameter count mismatch"
    );

    // Step 2: Download parameter blobs using the full production pipeline.
    // Workers call blob_ticket_param_request_task (production code), then send tickets
    // to the main loop. The main loop calls start_download/poll_next which exercises
    // DownloadManager, BandwidthTracker, and ConnectionMonitor integration.
    info!(
        "Downloader {endpoint_id}: downloading {} parameters via production pipeline from {} sharers...",
        param_names.len(),
        sharer_ids.len(),
    );

    // Shared work queue: workers pull param names from here
    let work_queue: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(param_names.clone()));

    // Channel for workers to send download requests to main loop
    let (request_tx, mut request_rx) =
        tokio::sync::mpsc::channel::<DownloadRequest>(max_concurrent * 2);

    // Spawn N worker tasks that each loop: pick param → blob_ticket_param_request_task → send to main
    for worker_id in 0..max_concurrent {
        let work_q = work_queue.clone();
        let pm = peer_manager.clone();
        let rtr = router.clone();
        let tx = request_tx.clone();
        let cancel = cancel.clone();
        let ep_id = endpoint_id;

        tokio::spawn(async move {
            loop {
                // Grab next param from work queue
                let name = {
                    let mut q = work_q.lock().await;
                    if q.is_empty() {
                        break;
                    }
                    q.remove(0)
                };

                if cancel.is_cancelled() {
                    break;
                }

                // Use production blob_ticket_param_request_task: handles peer selection,
                // retry logic, report_success (returns peer to pool after ticket request)
                let request_type = ModelRequestType::Parameter(name.clone());
                let result = blob_ticket_param_request_task(
                    request_type,
                    rtr.clone(),
                    pm.clone(),
                    cancel.clone(),
                )
                .await;

                match result {
                    Ok((ticket, _)) => {
                        let peer_id = ticket.addr().id;
                        info!(
                            "Downloader {ep_id} worker-{worker_id}: got ticket for '{name}' from {}",
                            peer_id.fmt_short()
                        );

                        let (done_tx, done_rx) = oneshot::channel();
                        let req = DownloadRequest {
                            name: name.clone(),
                            ticket,
                            peer_id,
                            start_time: Instant::now(),
                            done_tx,
                        };

                        if tx.send(req).await.is_err() {
                            break; // main loop gone
                        }

                        // Wait for main loop to signal download completion
                        let _ = done_rx.await;
                    }
                    Err(e) => {
                        error!(
                            "Downloader {ep_id} worker-{worker_id}: failed to get ticket for '{name}': {e}"
                        );
                        break;
                    }
                }
            }
        });
    }

    // Drop our sender so request_rx closes when all workers are done
    drop(request_tx);

    // Main loop: single task owns &mut network for start_download + poll_next
    // Keyed by hash → Vec because FakeStore can produce identical blobs for
    // different parameters (same size = same content = same hash).
    let mut in_flight: HashMap<Hash, Vec<InFlightDownload>> = HashMap::new();
    let mut completed = 0usize;
    let mut tag_counter = 0u64;

    loop {
        if completed >= expected_param_count {
            break;
        }

        select! {
            _ = cancel.cancelled() => {
                return Err(anyhow::anyhow!("Cancelled during parameter downloads"));
            }
            // Accept new download requests from workers
            req = request_rx.recv() => {
                match req {
                    Some(download_req) => {
                        tag_counter += 1;
                        let tag_name = format!("param-dl-{tag_counter}");
                        let tag = Tag::from(tag_name.clone());
                        let hash = download_req.ticket.hash();

                        info!(
                            "Downloader {endpoint_id}: starting download for '{}' hash {} from {}",
                            download_req.name, hash.fmt_short(), download_req.peer_id.fmt_short()
                        );

                        network.start_download(
                            download_req.ticket,
                            tag,
                            DownloadType::ModelSharing(ModelRequestType::Parameter(download_req.name.clone())),
                        );

                        in_flight.entry(hash).or_default().push(InFlightDownload {
                            name: download_req.name,
                            tag_name,
                            peer_id: download_req.peer_id,
                            start_time: download_req.start_time,
                            done_tx: download_req.done_tx,
                        });
                    }
                    None => {
                        // All workers done sending requests; wait for remaining in-flight
                        in_flight.retain(|_, v| !v.is_empty());
                        if in_flight.is_empty() {
                            break;
                        }
                    }
                }
            }
            // Poll network for download progress and completions
            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::DownloadComplete(result))) => {
                        let hash = result.hash;
                        let flight = in_flight.get_mut(&hash).and_then(|v| v.pop());
                        if let Some(flight) = flight {
                            let duration = flight.start_time.elapsed();
                            let peer_bw = connection_monitor
                                .get_all_connections()
                                .iter()
                                .find(|c| c.endpoint_id == flight.peer_id)
                                .map(|c| format_bandwidth(&c.bandwidth))
                                .unwrap_or_else(|| "unknown".to_string());

                            info!(
                                "Downloader {endpoint_id}: '{}' from {} completed in {duration:?} (peer bw: {peer_bw})",
                                flight.name, flight.peer_id.fmt_short(),
                            );

                            completed += 1;
                            param_reports.push(ParamDownloadReport {
                                name: flight.name,
                                duration,
                                from_peer: flight.peer_id,
                                peer_bandwidth_at_download: peer_bw,
                            });

                            // Signal worker that download is done
                            let _ = flight.done_tx.send(true);

                            // Delete tag to free MemStore memory (GC reclaims blob)
                            if let Err(e) = network.delete_tag(&flight.tag_name).await {
                                warn!("Failed to delete tag {}: {e}", flight.tag_name);
                            }
                        }

                        if completed % 10 == 0 || completed == expected_param_count {
                            print_peer_status(
                                &connection_monitor,
                                &format!(
                                    "Downloader {endpoint_id} after {completed}/{expected_param_count} params"
                                ),
                            );
                        }
                    }
                    Ok(Some(NetworkEvent::DownloadFailed(f))) => {
                        // FakeStore blobs are raw zeros, not valid postcard. The download
                        // itself succeeded (bytes transferred, bandwidth tracked via
                        // on_download_update/BandwidthTracker) but deserialization failed.
                        // poll_next no longer zeroes bandwidth for deserialization failures
                        // (transfer_failed=false), so peer selection stays accurate.
                        let hash = f.blob_ticket.hash();
                        let peer_id = f.blob_ticket.addr().id;

                        let flight = in_flight.get_mut(&hash).and_then(|v| v.pop());
                        if let Some(flight) = flight {
                            let duration = flight.start_time.elapsed();

                            let peer_bw_display = connection_monitor
                                .get_all_connections()
                                .iter()
                                .find(|c| c.endpoint_id == peer_id)
                                .map(|c| format_bandwidth(&c.bandwidth))
                                .unwrap_or_else(|| "unknown".to_string());

                            info!(
                                "Downloader {endpoint_id}: '{}' from {} download failed (expected: FakeStore deserialization) in {duration:?} (peer bw: {peer_bw_display})",
                                flight.name, flight.peer_id.fmt_short(),
                            );

                            completed += 1;
                            param_reports.push(ParamDownloadReport {
                                name: flight.name,
                                duration,
                                from_peer: flight.peer_id,
                                peer_bandwidth_at_download: peer_bw_display,
                            });

                            // Signal worker that download is done
                            let _ = flight.done_tx.send(true);

                            // Delete tag to free MemStore memory
                            if let Err(e) = network.delete_tag(&f.tag.to_string()).await {
                                warn!("Failed to delete tag: {e}");
                            }
                        }

                        if completed % 10 == 0 || completed == expected_param_count {
                            print_peer_status(
                                &connection_monitor,
                                &format!(
                                    "Downloader {endpoint_id} after {completed}/{expected_param_count} params"
                                ),
                            );
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("Downloader {endpoint_id}: network error: {e:#}");
                    }
                }
            }
        }
    }

    let total_duration = overall_start.elapsed();
    info!(
        "Downloader {endpoint_id}: {completed}/{} parameters downloaded in {total_duration:?}",
        param_names.len()
    );

    Ok(DownloaderReport {
        endpoint_id,
        total_duration,
        config_request_time,
        param_reports,
    })
}

#[derive(Debug)]
struct ParamDownloadReport {
    name: String,
    duration: Duration,
    from_peer: PublicKey,
    peer_bandwidth_at_download: String,
}

#[derive(Debug)]
struct DownloaderReport {
    endpoint_id: EndpointId,
    total_duration: Duration,
    config_request_time: Duration,
    param_reports: Vec<ParamDownloadReport>,
}

fn print_report(reports: &[DownloaderReport], param_size_bytes: usize) {
    let separator = "=".repeat(70);
    println!("\n{separator}");
    println!("  MODEL SHARING TEST RESULTS");
    println!("{separator}");

    for report in reports {
        println!("\n--- Downloader {} ---", report.endpoint_id);
        println!("  Config request time: {:?}", report.config_request_time);
        println!("  Total download time: {:?}", report.total_duration);
        println!("  Parameters downloaded: {}", report.param_reports.len());

        if !report.param_reports.is_empty() {
            let total_bytes = report.param_reports.len() as f64 * param_size_bytes as f64;
            let avg_bw = total_bytes / report.total_duration.as_secs_f64();
            println!(
                "  Average bandwidth: {:.2} MB/s",
                avg_bw / (1024.0 * 1024.0)
            );

            // Per-peer breakdown
            let mut per_peer: HashMap<String, (usize, Duration)> = HashMap::new();
            for pr in &report.param_reports {
                let key = pr.from_peer.fmt_short().to_string();
                let entry = per_peer.entry(key).or_insert((0, Duration::ZERO));
                entry.0 += 1;
                entry.1 += pr.duration;
            }

            println!("  Per-peer breakdown:");
            for (peer, (count, total_time)) in &per_peer {
                let peer_bytes = *count as f64 * param_size_bytes as f64;
                let peer_bw = peer_bytes / total_time.as_secs_f64();
                println!(
                    "    {peer}: {count} params, total {total_time:?}, avg {:.2} MB/s",
                    peer_bw / (1024.0 * 1024.0)
                );
            }

            // Slowest/fastest params
            let mut sorted_params: Vec<_> = report.param_reports.iter().collect();
            sorted_params.sort_by_key(|p| p.duration);
            if let Some(fastest) = sorted_params.first() {
                println!(
                    "  Fastest param: '{}' in {:?}",
                    fastest.name, fastest.duration
                );
            }
            if let Some(slowest) = sorted_params.last() {
                println!(
                    "  Slowest param: '{}' in {:?}",
                    slowest.name, slowest.duration
                );
            }

            // Peer selection log
            println!("\n  Peer Selection Log:");
            for pr in &report.param_reports {
                println!(
                    "    {} -> peer {} (bw: {}, took {:?})",
                    pr.name,
                    pr.from_peer.fmt_short(),
                    pr.peer_bandwidth_at_download,
                    pr.duration,
                );
            }
        }
    }
    println!("\n{separator}");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    // Initialize logging
    let _logger = psyche_tui::logging()
        .with_output(LogOutput::Console)
        .init()?;

    let discovery_mode: DiscoveryMode = args
        .discovery_mode
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let relay_kind: RelayKind = args
        .relay_kind
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let param_names = generate_parameter_names(args.num_parameters);
    let param_size_bytes = args.parameter_size_mb * 1024 * 1024;

    println!("Model Sharing Test Configuration:");
    println!("  Sharers:              {}", args.num_sharers);
    println!("  Downloaders:          {}", args.num_downloaders);
    println!("  Parameters:           {}", args.num_parameters);
    println!("  Parameter size:       {} MB", args.parameter_size_mb);
    println!("  Max concurrent DLs:   {}", args.max_concurrent_downloads);
    println!("  Discovery mode:       {discovery_mode:?}");
    println!("  Relay kind:           {relay_kind:?}");
    if args.slow_sharers > 0 {
        println!(
            "  Slow sharers:         {} (throttled to {} KB/s)",
            args.slow_sharers, args.slow_sharer_rate_kb
        );
    }
    println!(
        "  Total data:           {:.1} GB",
        (args.num_parameters * args.parameter_size_mb) as f64 / 1024.0
    );
    println!();

    let cancel = CancellationToken::new();

    let num_fast = args.num_sharers.saturating_sub(args.slow_sharers);
    let num_slow = args.slow_sharers.min(args.num_sharers);

    // Create FakeStore for fast sharers (unlimited bandwidth)
    let fast_store = FakeStore::builder()
        .with_unique_blobs(args.num_parameters, param_size_bytes as u64)
        .build();
    info!(
        "Created fast FakeStore with {} blobs of {} MB each",
        args.num_parameters, args.parameter_size_mb
    );

    // Create FakeStore for slow sharers (throttled bandwidth)
    let slow_store = if num_slow > 0 {
        let store = FakeStore::builder()
            .with_unique_blobs(args.num_parameters, param_size_bytes as u64)
            .with_throttle(
                std::num::NonZeroU64::new(args.slow_sharer_rate_kb * 1024)
                    .expect("slow_sharer_rate_kb must be > 0"),
            ) // KB/s -> bytes/s
            .build();
        info!(
            "Created slow FakeStore throttled to {} KB/s",
            args.slow_sharer_rate_kb
        );
        Some(store)
    } else {
        None
    };

    // Create sharer peers
    let mut sharer_handles = Vec::new();
    let mut sharer_ids = Vec::new();

    for i in 0..args.num_sharers {
        let is_slow = i >= num_fast;
        let store = if is_slow {
            slow_store.as_ref().unwrap()
        } else {
            &fast_store
        };
        let label = if is_slow {
            format!("Sharer-{i}-SLOW")
        } else {
            format!("Sharer-{i}")
        };
        let network = create_sharer_peer(&label, discovery_mode, relay_kind, store).await?;
        let eid = network.endpoint_id();
        if is_slow {
            info!(
                "Sharer-{i} ({}) is SLOW (throttled to {} KB/s)",
                eid.fmt_short(),
                args.slow_sharer_rate_kb
            );
        } else {
            info!("Sharer-{i} ({}) is FAST (unlimited)", eid.fmt_short());
        }
        sharer_ids.push(eid);
        let param_names = param_names.clone();
        let cancel = cancel.clone();
        let fs = store.clone();
        sharer_handles.push(tokio::spawn(async move {
            run_sharer(network, fs, param_names, param_size_bytes, cancel).await
        }));
    }

    // Give sharers a moment to set up before downloaders connect
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create downloader peers
    let mut downloader_handles = Vec::new();

    for i in 0..args.num_downloaders {
        let network = create_peer(&format!("Downloader-{i}"), discovery_mode, relay_kind).await?;
        let sharer_ids = sharer_ids.clone();
        let cancel = cancel.clone();
        let expected_param_count = args.num_parameters;
        let max_concurrent = args.max_concurrent_downloads;
        downloader_handles.push(tokio::spawn(async move {
            run_downloader(
                network,
                sharer_ids,
                expected_param_count,
                param_size_bytes,
                max_concurrent,
                cancel,
            )
            .await
        }));
    }

    // Wait for all downloaders to finish
    let mut reports = Vec::new();
    for handle in downloader_handles {
        match handle.await? {
            Ok(report) => reports.push(report),
            Err(e) => error!("Downloader failed: {e:#}"),
        }
    }

    // Cancel sharers
    cancel.cancel();
    for handle in sharer_handles {
        let _ = handle.await;
    }

    // Print results
    print_report(&reports, param_size_bytes);

    Ok(())
}
