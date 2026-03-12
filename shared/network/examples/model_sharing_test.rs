use anyhow::Result;
use clap::{Parser, ValueEnum};
use iroh::EndpointAddr;
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

#[derive(Debug, Clone, ValueEnum)]
enum RunMode {
    /// Run everything in one process (default, original behavior)
    All,
    /// Run as a sharer only — prints endpoint address JSON to stdout for downloaders
    Sharer,
    /// Run as a downloader only — requires --sharer-addr
    Downloader,
}

#[derive(Parser, Debug)]
#[command(name = "model_sharing_test")]
#[command(about = "Test harness for P2P model sharing flow")]
struct CliArgs {
    /// Run mode: "all" (default), "sharer", or "downloader"
    #[clap(long, value_enum, default_value_t = RunMode::All)]
    mode: RunMode,

    /// JSON-encoded sharer endpoint address (required in downloader mode).
    /// Can also be a path to a file containing the JSON.
    #[clap(long)]
    sharer_addr: Option<String>,

    #[clap(long, default_value_t = 1)]
    num_sharers: usize,

    #[clap(long, default_value_t = 2)]
    num_downloaders: usize,

    #[clap(long, default_value_t = 300)]
    num_parameters: usize,

    /// Size of each parameter in MB
    #[clap(long, default_value_t = 1000)]
    parameter_size_mb: usize,

    #[clap(long, default_value_t = 4)]
    max_concurrent_downloads: usize,

    /// Discovery mode: "local" or "n0"
    #[clap(long, default_value = "local")]
    discovery_mode: String,

    /// Relay kind: "disabled", "psyche", or "n0"
    #[clap(long, default_value = "psyche")]
    relay_kind: String,

    /// Number of sharers that should be slow (throttled). These will be the last N sharers.
    #[clap(long, default_value_t = 0)]
    slow_sharers: usize,

    /// Bandwidth limit for slow sharers in KB/s
    #[clap(long, default_value_t = 100)]
    slow_sharer_rate_kb: u64,

    /// Force all blob downloads to go through the relay (strip direct IP addresses from tickets)
    #[clap(long, default_value_t = false)]
    relay_only: bool,
}

/// Required by NetworkConnection generics but unused in this example.
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

async fn create_peer(
    label: &str,
    discovery_mode: DiscoveryMode,
    relay_kind: RelayKind,
    fake_store: Option<&FakeStore>,
    relay_only: bool,
) -> Result<NC> {
    let metrics = Arc::new(ClientMetrics::new(None, None));
    let network = if let Some(store) = fake_store {
        let store: iroh_blobs::api::Store = std::ops::Deref::deref(store).clone();
        NC::init_with_blobs_store(
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
            relay_only,
        )
        .await?
    } else {
        NC::init(
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
        .await?
    };

    info!("{label} initialized: {}", network.endpoint_id());
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

async fn run_sharer(
    mut network: NC,
    fake_store: FakeStore,
    param_names: Vec<String>,
    param_size_bytes: usize,
    relay_only: bool,
    cancel: CancellationToken,
) -> Result<()> {
    let endpoint_id = network.endpoint_id();
    info!(
        "Sharer {endpoint_id}: preparing {} params ({} MB each)",
        param_names.len(),
        param_size_bytes / (1024 * 1024)
    );

    let mut endpoint_addr = network.endpoint_addr().await;
    if relay_only {
        // Strip direct IP addresses so downloaders must connect via relay
        endpoint_addr.addrs.retain(|a| a.is_relay());
        info!(
            "Sharer {endpoint_id}: relay-only mode, stripped direct addresses. Addr: {endpoint_addr:?}"
        );
    }
    let fake_hashes = fake_store.blobs().list().hashes().await?;
    let param_tickets: HashMap<String, BlobTicket> = param_names
        .iter()
        .zip(fake_hashes.iter())
        .map(|(name, hash)| {
            let ticket = BlobTicket::new(endpoint_addr.clone(), *hash, BlobFormat::Raw);
            (name.clone(), ticket)
        })
        .collect();

    // Model config blob stored in FakeStore so BlobsProtocol can serve it
    let config = TransmittableModelConfig::new(
        r#"{"model_type": "test", "num_layers": 10}"#.to_string(),
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":null,"post_processor":null,"decoder":null,"model":{"type":"BPE","dropout":null,"unk_token":null,"continuing_subword_prefix":null,"end_of_word_suffix":null,"fuse_unk":false,"byte_fallback":false,"ignore_merges":false,"vocab":{},"merges":[]}}"#.to_string(),
        param_names.clone(),
    );
    let config_bytes = postcard::to_allocvec(&TransmittableDownload::ModelConfig(config))?;
    let config_blob = fake_store
        .blobs()
        .add_bytes(config_bytes)
        .with_named_tag(Tag::from("model-config-share"))
        .await?;
    let config_ticket =
        BlobTicket::new(endpoint_addr.clone(), config_blob.hash, config_blob.format);

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
                        let _ = reply_tx.send(Ok(config_ticket.clone()));
                    }
                    Ok(Some(_)) => {}
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

struct DownloadRequest {
    name: String,
    ticket: BlobTicket,
    peer_id: EndpointId,
    start_time: Instant,
    done_tx: oneshot::Sender<bool>,
}

struct InFlightDownload {
    name: String,
    tag_name: String,
    peer_id: EndpointId,
    start_time: Instant,
    done_tx: oneshot::Sender<bool>,
}

/// Records a completed download and signals the worker.
/// Returns the tag name for cleanup.
fn record_completion(
    flight: InFlightDownload,
    endpoint_id: EndpointId,
    completed: &mut usize,
    param_reports: &mut Vec<ParamDownloadReport>,
) -> String {
    let duration = flight.start_time.elapsed();
    info!(
        "Downloader {endpoint_id}: '{}' from {} in {duration:?}",
        flight.name,
        flight.peer_id.fmt_short(),
    );

    *completed += 1;
    param_reports.push(ParamDownloadReport {
        name: flight.name,
        duration,
        from_peer: flight.peer_id,
    });

    let _ = flight.done_tx.send(true);
    flight.tag_name
}

async fn run_downloader(
    mut network: NC,
    sharer_ids: Vec<EndpointId>,
    expected_param_count: usize,
    max_concurrent: usize,
    param_size_bytes: usize,
    cancel: CancellationToken,
) -> Result<DownloaderReport> {
    let endpoint_id = network.endpoint_id();
    let connection_monitor = network.connection_monitor();
    let router = network.router();

    let peer_manager = Arc::new(PeerManagerHandle::new(
        3,
        cancel.clone(),
        connection_monitor.clone(),
    ));
    peer_manager.set_peers(sharer_ids.clone());

    let overall_start = Instant::now();
    let mut param_reports: Vec<ParamDownloadReport> = Vec::new();

    // Step 1: Download model config
    let config_start = Instant::now();
    let (config_ticket, _) = blob_ticket_param_request_task(
        ModelRequestType::Config,
        router.clone(),
        peer_manager.clone(),
        cancel.clone(),
    )
    .await?;

    network.start_download(
        config_ticket,
        Tag::from("model-config"),
        DownloadType::ModelSharing(ModelRequestType::Config),
    );

    let param_names = loop {
        select! {
            _ = cancel.cancelled() => {
                return Err(anyhow::anyhow!("Cancelled while downloading config"));
            }
            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::DownloadComplete(result))) => {
                        if let TransmittableDownload::ModelConfig(config) = result.data {
                            info!("Downloader {endpoint_id}: config downloaded with {} params in {:?}",
                                config.parameter_names.len(), config_start.elapsed());
                            break config.parameter_names;
                        }
                    }
                    Ok(Some(NetworkEvent::DownloadFailed(f))) => {
                        return Err(anyhow::anyhow!("Config download failed: {}", f.error));
                    }
                    Ok(_) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    };
    let config_request_time = config_start.elapsed();

    assert_eq!(
        param_names.len(),
        expected_param_count,
        "Config parameter count mismatch"
    );

    // Step 2: Download parameters via worker pool + main download loop
    info!(
        "Downloader {endpoint_id}: downloading {} params from {} sharers...",
        param_names.len(),
        sharer_ids.len(),
    );

    let work_queue: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(param_names.clone()));
    let (request_tx, mut request_rx) =
        tokio::sync::mpsc::channel::<DownloadRequest>(max_concurrent * 2);

    // Workers: pick param -> request ticket from peer -> send to main loop
    for worker_id in 0..max_concurrent {
        let work_q = work_queue.clone();
        let pm = peer_manager.clone();
        let rtr = router.clone();
        let tx = request_tx.clone();
        let cancel = cancel.clone();
        let ep_id = endpoint_id;

        tokio::spawn(async move {
            loop {
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

                let request_type = ModelRequestType::Parameter(name.clone());
                match blob_ticket_param_request_task(
                    request_type,
                    rtr.clone(),
                    pm.clone(),
                    cancel.clone(),
                )
                .await
                {
                    Ok((ticket, _)) => {
                        let (done_tx, done_rx) = oneshot::channel();
                        let req = DownloadRequest {
                            peer_id: ticket.addr().id,
                            name,
                            ticket,
                            start_time: Instant::now(),
                            done_tx,
                        };
                        if tx.send(req).await.is_err() {
                            break;
                        }
                        let _ = done_rx.await;
                    }
                    Err(e) => {
                        error!(
                            "Downloader {ep_id} worker-{worker_id}: ticket request failed for '{name}': {e}"
                        );
                        break;
                    }
                }
            }
        });
    }
    drop(request_tx);

    // Main loop: owns &mut network for start_download + poll_next.
    // Keyed by hash -> Vec because FakeStore can produce identical hashes for same-size blobs.
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
            req = request_rx.recv() => {
                match req {
                    Some(dl_req) => {
                        tag_counter += 1;
                        let tag_name = format!("param-dl-{tag_counter}");
                        let hash = dl_req.ticket.hash();

                        network.start_download(
                            dl_req.ticket,
                            Tag::from(tag_name.clone()),
                            DownloadType::ModelSharing(ModelRequestType::Parameter(dl_req.name.clone())),
                        );

                        in_flight.entry(hash).or_default().push(InFlightDownload {
                            name: dl_req.name,
                            tag_name,
                            peer_id: dl_req.peer_id,
                            start_time: dl_req.start_time,
                            done_tx: dl_req.done_tx,
                        });
                    }
                    None => {
                        in_flight.retain(|_, v| !v.is_empty());
                        if in_flight.is_empty() {
                            break;
                        }
                    }
                }
            }
            event = network.poll_next() => {
                match &event {
                    Ok(Some(NetworkEvent::DownloadComplete(r))) => {
                        let hash = r.hash;
                        if let Some(flight) = in_flight.get_mut(&hash).and_then(|v| v.pop()) {
                            let tag_to_delete = record_completion(
                                flight, endpoint_id,
                                &mut completed, &mut param_reports,
                            );
                            if let Err(e) = network.delete_tag(&tag_to_delete).await {
                                warn!("Failed to delete tag {tag_to_delete}: {e}");
                            }
                        }
                    }
                    Ok(Some(NetworkEvent::DownloadFailed(f))) => {
                        let hash = f.blob_ticket.hash();
                        let tag = f.tag.to_string();
                        if let Some(flight) = in_flight.get_mut(&hash).and_then(|v| v.pop()) {
                            if f.transfer_failed {
                                // Real failure (timeout, network error) — re-queue for retry
                                warn!(
                                    "Downloader {endpoint_id}: transfer failed for '{}' from {}, re-queuing",
                                    flight.name, flight.peer_id.fmt_short(),
                                );
                                // Push to work queue BEFORE signaling done to avoid race where
                                // the worker wakes up, sees empty queue, and exits.
                                work_queue.lock().await.push(flight.name);
                                let _ = flight.done_tx.send(true);
                            } else {
                                // Deserialization error — transfer succeeded (FakeStore data can't deserialize).
                                // The core sets bandwidth to 0 on any DownloadFailed, but the
                                // BandwidthTracker had the real value from Progress events.
                                // Restore it using the tracker's last reading.
                                let peer_id = f.blob_ticket.addr().id;
                                let tracker_bw = network.bandwidth_tracker_peer_bandwidth(&peer_id);
                                let elapsed = flight.start_time.elapsed().as_secs_f64();
                                let manual_bw = if elapsed > 0.0 {
                                    param_size_bytes as f64 / elapsed
                                } else {
                                    0.0
                                };
                                info!(
                                    "Bandwidth for '{}' from {}: tracker={}, manual={:.1} KB/s",
                                    flight.name,
                                    peer_id.fmt_short(),
                                    format_bandwidth(&tracker_bw),
                                    manual_bw / 1024.0,
                                );
                                // Restore from tracker (preferred), fallback to manual
                                let restore_bw = match tracker_bw {
                                    PeerBandwidth::Measured(bw) => PeerBandwidth::Measured(bw),
                                    PeerBandwidth::NotMeasured if manual_bw > 0.0 => {
                                        PeerBandwidth::Measured(manual_bw)
                                    }
                                    _ => PeerBandwidth::NotMeasured,
                                };
                                network.connection_monitor().update_peer_bandwidth(
                                    &peer_id,
                                    restore_bw,
                                );
                                record_completion(
                                    flight, endpoint_id,
                                    &mut completed, &mut param_reports,
                                );
                            }
                            if let Err(e) = network.delete_tag(&tag).await {
                                warn!("Failed to delete tag {tag}: {e}");
                            }
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(e) => {
                        error!("Downloader {endpoint_id}: network error: {e:#}");
                    }
                }

                if completed % 10 == 0 || completed == expected_param_count {
                    if completed > 0 {
                        print_peer_status(
                            &connection_monitor,
                            &format!("Downloader {endpoint_id} {completed}/{expected_param_count}"),
                        );
                    }
                }
            }
        }
    }

    let total_duration = overall_start.elapsed();
    info!(
        "Downloader {endpoint_id}: {completed}/{} params in {total_duration:?}",
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

        if report.param_reports.is_empty() {
            continue;
        }

        let total_bytes = report.param_reports.len() as f64 * param_size_bytes as f64;
        let avg_bw = total_bytes / report.total_duration.as_secs_f64();
        println!(
            "  Average bandwidth: {:.2} MB/s",
            avg_bw / (1024.0 * 1024.0)
        );

        let mut per_peer: HashMap<String, (usize, Duration)> = HashMap::new();
        for pr in &report.param_reports {
            let entry = per_peer
                .entry(pr.from_peer.fmt_short().to_string())
                .or_insert((0, Duration::ZERO));
            entry.0 += 1;
            entry.1 += pr.duration;
        }

        println!("  Per-peer breakdown:");
        for (peer, (count, total_time)) in &per_peer {
            let peer_bw = (*count as f64 * param_size_bytes as f64) / total_time.as_secs_f64();
            println!(
                "    {peer}: {count} params, total {total_time:?}, avg {:.2} MB/s",
                peer_bw / (1024.0 * 1024.0)
            );
        }

        let mut sorted_params: Vec<_> = report.param_reports.iter().collect();
        sorted_params.sort_by_key(|p| p.duration);
        if let (Some(fastest), Some(slowest)) = (sorted_params.first(), sorted_params.last()) {
            println!("  Fastest: '{}' in {:?}", fastest.name, fastest.duration);
            println!("  Slowest: '{}' in {:?}", slowest.name, slowest.duration);
        }
    }
    println!("\n{separator}");
}

/// Parse sharer address from a JSON string or a file path containing JSON.
fn parse_sharer_addr(raw: &str) -> Result<EndpointAddr> {
    // Try parsing as JSON first
    if let Ok(addr) = serde_json::from_str::<EndpointAddr>(raw) {
        return Ok(addr);
    }
    // Try reading as a file path
    let content = std::fs::read_to_string(raw)
        .map_err(|e| anyhow::anyhow!("Failed to read sharer-addr file '{raw}': {e}"))?;
    serde_json::from_str::<EndpointAddr>(content.trim())
        .map_err(|e| anyhow::anyhow!("Failed to parse sharer-addr JSON: {e}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();
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
    let relay_only = args.relay_only;

    println!("Model Sharing Test Configuration:");
    println!("  Mode:                 {:?}", args.mode);
    println!("  Parameters:           {}", args.num_parameters);
    println!("  Parameter size:       {} MB", args.parameter_size_mb);
    println!("  Max concurrent DLs:   {}", args.max_concurrent_downloads);
    println!("  Discovery mode:       {discovery_mode:?}");
    println!("  Relay kind:           {relay_kind:?}");
    if relay_only {
        println!(
            "  Relay-only mode:      ENABLED (direct IP transports disabled, all traffic via relay)"
        );
    }
    println!(
        "  Total data:           {:.1} GB",
        (args.num_parameters * args.parameter_size_mb) as f64 / 1024.0
    );
    println!();

    let cancel = CancellationToken::new();

    match args.mode {
        RunMode::Sharer => {
            // Single sharer mode: create sharer, write endpoint addr to stdout and optionally file
            let store = FakeStore::builder()
                .with_unique_blobs(args.num_parameters, param_size_bytes as u64)
                .build();
            let network = create_peer(
                "Sharer-0",
                discovery_mode,
                relay_kind,
                Some(&store),
                relay_only,
            )
            .await?;

            let endpoint_addr = network.endpoint_addr().await;
            let addr_json = serde_json::to_string(&endpoint_addr)?;

            // Print with marker so it can be parsed from logs
            println!("SHARER_ADDR_JSON:{addr_json}");

            // Also write to /tmp/sharer_addr.json for Docker volume sharing
            if let Err(e) = std::fs::write("/tmp/sharer_addr.json", &addr_json) {
                warn!("Could not write sharer addr to /tmp/sharer_addr.json: {e}");
            }

            info!("Sharer running, endpoint: {}", network.endpoint_id());
            run_sharer(
                network,
                store,
                param_names,
                param_size_bytes,
                relay_only,
                cancel,
            )
            .await?;
        }

        RunMode::Downloader => {
            let sharer_addr_raw = args
                .sharer_addr
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--sharer-addr is required in downloader mode"))?;
            let sharer_addr = parse_sharer_addr(sharer_addr_raw)?;
            let sharer_id = sharer_addr.id;
            info!("Downloader targeting sharer: {}", sharer_id.fmt_short());

            let mut downloader_handles = Vec::new();
            for i in 0..args.num_downloaders {
                let network = create_peer(
                    &format!("Downloader-{i}"),
                    discovery_mode,
                    relay_kind,
                    None,
                    relay_only,
                )
                .await?;
                let sharer_ids = vec![sharer_id];
                let cancel = cancel.clone();
                let expected = args.num_parameters;
                let max_concurrent = args.max_concurrent_downloads;
                downloader_handles.push(tokio::spawn(async move {
                    run_downloader(
                        network,
                        sharer_ids,
                        expected,
                        max_concurrent,
                        param_size_bytes,
                        cancel,
                    )
                    .await
                }));
            }

            let mut reports = Vec::new();
            for handle in downloader_handles {
                match handle.await? {
                    Ok(report) => reports.push(report),
                    Err(e) => error!("Downloader failed: {e:#}"),
                }
            }

            cancel.cancel();
            print_report(&reports, param_size_bytes);
        }

        RunMode::All => {
            // Original behavior: everything in one process
            println!("  Sharers:              {}", args.num_sharers);
            println!("  Downloaders:          {}", args.num_downloaders);
            if args.slow_sharers > 0 {
                println!(
                    "  Slow sharers:         {} (throttled to {} KB/s)",
                    args.slow_sharers, args.slow_sharer_rate_kb
                );
            }
            println!();

            let num_fast = args.num_sharers.saturating_sub(args.slow_sharers);
            let mut sharer_handles = Vec::new();
            let mut sharer_ids = Vec::new();

            for i in 0..args.num_sharers {
                let is_slow = i >= num_fast;
                let store = if is_slow {
                    FakeStore::builder()
                        .with_unique_blobs(args.num_parameters, param_size_bytes as u64)
                        .with_throttle(
                            std::num::NonZeroU64::new(args.slow_sharer_rate_kb * 1024)
                                .expect("slow_sharer_rate_kb must be > 0"),
                        )
                        .build()
                } else {
                    FakeStore::builder()
                        .with_unique_blobs(args.num_parameters, param_size_bytes as u64)
                        .build()
                };
                let label = if is_slow {
                    format!("Sharer-{i}-SLOW")
                } else {
                    format!("Sharer-{i}")
                };
                let network =
                    create_peer(&label, discovery_mode, relay_kind, Some(&store), relay_only)
                        .await?;
                sharer_ids.push(network.endpoint_id());

                let param_names = param_names.clone();
                let cancel = cancel.clone();
                sharer_handles.push(tokio::spawn(async move {
                    run_sharer(
                        network,
                        store,
                        param_names,
                        param_size_bytes,
                        relay_only,
                        cancel,
                    )
                    .await
                }));
            }

            tokio::time::sleep(Duration::from_secs(2)).await;

            let mut downloader_handles = Vec::new();
            for i in 0..args.num_downloaders {
                let network = create_peer(
                    &format!("Downloader-{i}"),
                    discovery_mode,
                    relay_kind,
                    None,
                    relay_only,
                )
                .await?;
                let sharer_ids = sharer_ids.clone();
                let cancel = cancel.clone();
                let expected = args.num_parameters;
                let max_concurrent = args.max_concurrent_downloads;
                downloader_handles.push(tokio::spawn(async move {
                    run_downloader(
                        network,
                        sharer_ids,
                        expected,
                        max_concurrent,
                        param_size_bytes,
                        cancel,
                    )
                    .await
                }));
            }

            let mut reports = Vec::new();
            for handle in downloader_handles {
                match handle.await? {
                    Ok(report) => reports.push(report),
                    Err(e) => error!("Downloader failed: {e:#}"),
                }
            }

            cancel.cancel();
            for handle in sharer_handles {
                let _ = handle.await;
            }

            print_report(&reports, param_size_bytes);
        }
    }

    Ok(())
}
