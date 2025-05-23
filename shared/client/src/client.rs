use crate::{
    state::{DistroBroadcastAndPayload, FinishedBroadcast, RunManager},
    Broadcast, BroadcastType, ClientTUIState, Finished, IntegrationTestLogMarker, RunInitConfig,
    RunInitConfigAndIO, TrainingResult, NC,
};
use anyhow::{bail, Error, Result};
use futures::future::join_all;
use psyche_coordinator::{Commitment, RunState};
use psyche_core::NodeIdentity;
use psyche_network::{
    allowlist, param_request_task, raw_p2p_verify, AuthenticatableIdentity, BlobTicket,
    DownloadComplete, ModelRequestType, NetworkConnection, NetworkEvent, NetworkTUIState,
    Networkable, NodeId, SharableModel, TransmittableDownload,
};
use psyche_watcher::{Backend, BackendWatcher};
use tokenizers::Tokenizer;

use rand::{seq::SliceRandom, thread_rng, RngCore};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::{mpsc, watch, Mutex, Notify},
    task::JoinHandle,
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};

pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NodeIdentity, A: AuthenticatableIdentity, B: Backend<T> + 'static> {
    rx_tui: watch::Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, A, B)>,
}

struct DownloadRetryInfo {
    retries: usize,
    retry_time: Option<Instant>,
    ticket: BlobTicket,
    tag: u32,
}

const MAX_DOWNLOAD_RETRIES: usize = 3;
const REBROADCAST_SHAREABLE: Duration = Duration::from_secs(2);
const DOWNLOAD_RETRY_BACKOFF_BASE: Duration = Duration::from_secs(2);
const DOWNLOAD_RETRY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const OPPROTUNISTIC_WITNESS_INTERVAL: Duration = Duration::from_millis(500);

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static, B: Backend<T> + 'static>
    Client<T, A, B>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: B,
        allowlist: allowlist::AllowDynamic,
        mut p2p: NC,
        init_config: RunInitConfig<T, A>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let (tx_tui, rx_tui) = watch::channel::<TUIStates>(Default::default());
        let req_tui_state = Arc::new(Notify::new());

        let identity = init_config.identity;
        let network_identity = init_config.network_identity.clone();
        let private_key = init_config.private_key.clone();
        let param_requests_cancel_token = CancellationToken::new();
        let join = tokio::spawn({
            let cancel = cancel.clone();
            let param_requests_cancel_token = param_requests_cancel_token.clone();
            let req_tui_state = req_tui_state.clone();
            async move {
                #[cfg(not(feature = "parallelism"))]
                if init_config.tensor_parallelism != 1 {
                    anyhow::bail!("Tensor parallelism was set but this build does not support it (must be built with --features=parallelism)")
                }

                let mut watcher = BackendWatcher::new(backend);

                // From Run
                let (tx_witness, mut rx_witness) = mpsc::unbounded_channel();
                let (tx_health_check, mut rx_health_check) = mpsc::unbounded_channel();
                let (tx_checkpoint, mut rx_checkpoint) = mpsc::unbounded_channel();
                let (tx_model, mut rx_model) = mpsc::unbounded_channel();
                let (tx_distro_result, mut rx_distro_result) = mpsc::unbounded_channel();
                let (tx_request_download, mut rx_request_download) = mpsc::unbounded_channel();
                let (tx_parameters_req, mut rx_parameters_req) = mpsc::unbounded_channel();
                let (tx_config, mut rx_config) = mpsc::unbounded_channel();
                let (tx_params_download, mut rx_params_download) = mpsc::unbounded_channel();
                let (tx_request_model_config, mut rx_request_model_config) =
                    mpsc::unbounded_channel();
                let (tx_broadcast_finished, mut rx_broadcast_finished) = mpsc::unbounded_channel();

                let max_concurrent_downloads = init_config.max_concurrent_parameter_requests;

                let mut run = RunManager::<T, A>::new(RunInitConfigAndIO {
                    init_config,

                    tx_witness,
                    tx_health_check,
                    tx_checkpoint,
                    tx_model,
                    tx_parameters_req,
                    tx_config,
                    tx_distro_result,
                    tx_request_download,
                    tx_request_model_config,
                    tx_broadcast_finished,
                });

                let mut retried_downloads: HashMap<psyche_network::Hash, DownloadRetryInfo> =
                    HashMap::new();
                let mut sharable_model = SharableModel::empty();
                let mut broadcasts = vec![];
                let mut broadcasts_rebroadcast_index = 0;
                let mut sharing_downloadable_interval = interval(REBROADCAST_SHAREABLE);
                let mut retry_check_interval = interval(DOWNLOAD_RETRY_CHECK_INTERVAL);
                let mut opprotunistic_witness_interval = interval(OPPROTUNISTIC_WITNESS_INTERVAL);
                debug!("Starting client loop");

                loop {
                    select! {
                        _ = cancel.cancelled() => {
                            break;
                        }

                         _ = req_tui_state.notified() => {
                            let network_tui_state = (&p2p).into();
                            let client_tui_state = (&run).into();
                            tx_tui.send((client_tui_state, network_tui_state))?;
                        },

                        state = watcher.poll_next() => {
                            let (old_state, new_state) = state?;
                            let old_run_state = old_state
                                .map(|s| s.run_state.to_string())
                                .unwrap_or_else(|| String::from(" - "));

                            trace!(
                                integration_test_log_marker = %IntegrationTestLogMarker::StateChange,
                                client_id = %identity,
                                old_state = old_run_state,
                                new_state = %new_state.run_state,
                                epoch = new_state.progress.epoch,
                                step = new_state.progress.step,
                                "applying state epoch {} step {} ({} -> {})",
                                new_state.progress.epoch,
                                new_state.progress.step,
                                old_run_state,
                                new_state.run_state
                            );

                            let connected_p2p_nodes: BTreeSet<_> = p2p.neighbors().collect();
                            {
                                let run_participating_node_ids: Vec<NodeId> = new_state
                                    .epoch_state
                                    .clients
                                    .iter()
                                    .map(|c| NodeId::from_bytes(c.id.get_p2p_public_key()).unwrap()).collect();
                                allowlist.set(run_participating_node_ids.iter().copied());

                                let my_node_id = p2p.node_id();

                                // only connect to peers after we become part of the set of current clients
                                if run_participating_node_ids.contains(&my_node_id) {
                                    const MAX_NUM_BOOTSTRAP_PEERS: usize = 3;
                                    // we only want to bootstrap gossip;
                                    // only connect to enough peers to bring our total peer count to at MOST MAX_NUM_BOOTSTRAP_PEERS.
                                    // if we already have that many or more, don't send any gossip joins
                                    // because gossip joins this way can force-disconnect other peers.
                                    let num_peers_to_add = MAX_NUM_BOOTSTRAP_PEERS.saturating_sub(connected_p2p_nodes.len());

                                    let mut to_connect = run_participating_node_ids
                                        .iter()
                                        .filter(|node_id| *node_id != &my_node_id)
                                        .filter(|node_id| !connected_p2p_nodes.contains(*node_id))
                                        .collect::<Vec<_>>();
                                    to_connect.shuffle(&mut thread_rng());
                                    let to_connect = to_connect.into_iter().take(num_peers_to_add).cloned().collect::<Vec<_>>();

                                    if !to_connect.is_empty() {
                                        info!(num_new_peers = to_connect.len(), "Connecting to new peers");
                                        p2p.add_peers(to_connect).await?;
                                    }
                                }
                            }

                            if old_state.map(|s| s.run_state) != Some(new_state.run_state) && new_state.run_state == RunState::RoundTrain {
                                trace!(num_peers = connected_p2p_nodes.len(), "Updating p2p");
                                let last_needed_step_blobs = new_state.progress.step.saturating_sub(2);
                                p2p.remove_blobs_with_tag_less_than(last_needed_step_blobs);
                                let p2p_info = get_p2p_info(&p2p).await?;
                                if let Err(e) = run.set_node_info(p2p_info) {
                                    warn!("failed to set p2p info: {e}");
                                }
                                broadcasts.retain(|(_, step)| *step >= last_needed_step_blobs);
                                sharable_model.clear_cache(); // IMPORTANT -- any cached blobs are now invalid
                            }

                            run.apply_state(*new_state).await?;
                        }

                        res = p2p.poll_next() => {
                            if let Some(message) = res? {
                                match message {
                                    NetworkEvent::MessageReceived((from, broadcast)) => {
                                        trace!("NetworkEvent::MessageReceived");
                                        if let Some(client) = watcher.get_client_for_p2p_public_key(from.as_bytes()) {
                                            if raw_p2p_verify(from.as_bytes(), &broadcast.commitment.data_hash, &broadcast.commitment.signature) {
                                                match &broadcast.data {
                                                    BroadcastType::TrainingResult(training_result) => {
                                                        trace!("Got training result gossip message from {from}: step {} batch id {}", broadcast.step, training_result.batch_id);
                                                    }
                                                    BroadcastType::Finished(_) => {
                                                        trace!("Got finished gossip message from {from}: step {}", broadcast.step);
                                                    }
                                                }
                                                run.apply_message(client.id, broadcast).await?;
                                            } else {
                                                debug!(from=from.fmt_short(), "Invalid signature on commitment from {}", from.fmt_short());
                                            }
                                        } else {
                                            warn!("Got broadcast from unknown client {}", from);
                                        }
                                    }
                                    NetworkEvent::DownloadComplete(DownloadComplete {
                                        data: download_data, hash, ..
                                    }) => {
                                        trace!("NetworkEvent::DownloadComplete({})", hex::encode(hash));
                                        if retried_downloads.remove(&hash).is_some() {
                                            debug!("Successfully downloaded previously failed blob {}", hex::encode(hash));
                                        }
                                        match download_data {
                                            TransmittableDownload::DistroResult(distro_result) => {
                                                trace!("Download complete: step {} batch id {}", distro_result.step, distro_result.batch_id);
                                                run.apply_distro_result(hash, distro_result, None).await;
                                            },
                                            TransmittableDownload::ModelParameter(parameter) => {
                                                sharable_model.add_parameter(parameter).await?;
                                                if sharable_model.is_download_complete() {
                                                    sharable_model.send_init_parameters()?;
                                                }
                                            },
                                            TransmittableDownload::ModelConfig(config) => {
                                                sharable_model.add_config(config)?;
                                                sharable_model.send_config()?;
                                            },
                                        }
                                    }
                                    NetworkEvent::DownloadFailed(dl) => {
                                        trace!("NetworkEvent::DownloadFailed({:?})", dl.error);
                                        let hash = dl.blob_ticket.hash();
                                        let info = retried_downloads.get(&hash);
                                        let retries = info.map(|i| i.retries).unwrap_or(0);

                                        if retries >= MAX_DOWNLOAD_RETRIES {
                                            warn!("Download failed (not retrying): {}", dl.error);
                                            retried_downloads.remove(&hash);
                                        } else {
                                            let backoff_duration = DOWNLOAD_RETRY_BACKOFF_BASE.mul_f32(2_f32.powi(retries as i32));
                                            let retry_time = Some(std::time::Instant::now() + backoff_duration);

                                            info!(
                                                "Download failed (will retry in {:?}): {}",
                                                backoff_duration,
                                                dl.error
                                            );

                                            retried_downloads.insert(hash, DownloadRetryInfo {
                                                retries: retries + 1,
                                                retry_time,
                                                ticket: dl.blob_ticket,
                                                tag: dl.tag,
                                            });
                                        }
                                    }
                                    NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx) => {
                                        // TODO: We should validate that the parameter is requested while we are in RunState::Warmup.
                                        trace!("NetworkEvent::ParameterRequest({parameter_name})");
                                        match sharable_model.get_transmittable_parameter(&parameter_name, &mut p2p, 0).await {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(ticket) => {
                                                info!(parameter = parameter_name, "Sending requested model parameter blob ticket");
                                                if let Err(e) = protocol_req_tx.send(Ok(ticket)) {
                                                    warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                                };
                                            }
                                        }
                                    },
                                    NetworkEvent::ModelConfigRequest(protocol_req_tx) => {
                                        trace!("NetworkEvent::ModelConfigRequest");
                                        match sharable_model.get_transmittable_config(&mut p2p, 0).await {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model config blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(config_ticket) => {
                                                info!("Sending requested model config blob ticket");
                                                if let Err(e) = protocol_req_tx.send(Ok(config_ticket)) {
                                                    warn!("Could not send model config blob ticket. Error: {e:?}");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Some(FinishedBroadcast { step, merkle, commitment_data_hash, proof, warmup }) = rx_broadcast_finished.recv() => {
                            trace!(
                                client_id = %identity, step = step,
                                "Broadcasting finished step merkle 0x{}",
                                hex::encode(merkle.inner),
                            );

                            let signature = network_identity.raw_p2p_sign(&private_key, &commitment_data_hash);
                            let commitment = Commitment { data_hash: commitment_data_hash, signature};
                            let training_result = Broadcast { step, proof, nonce: thread_rng().next_u32(), commitment, data: BroadcastType::Finished(Finished {
                                broadcast_merkle: merkle, warmup
                            })};

                            p2p.broadcast(&training_result).await?;
                            broadcasts.push((training_result.clone(), step));

                            // simulate us recving it & apply like anyone else's
                            run.apply_message(identity,  training_result).await?;
                        }

                        Some(DistroBroadcastAndPayload { step, batch_id, commitment_data_hash, proof, distro_result, original_distro_result }) = rx_distro_result.recv() => {

                            let transmittable_distro_result = TransmittableDownload::DistroResult(distro_result.clone());
                            let ticket = p2p.add_downloadable(transmittable_distro_result, step).await?;
                            let hash = ticket.hash();
                            trace!(
                                client_id = %identity, step = step,
                                "Broadcasting payload batch id {batch_id} hash 0x{}",
                                hex::encode(hash),
                            );

                            let signature = network_identity.raw_p2p_sign(&private_key, &commitment_data_hash);
                            let commitment = Commitment { data_hash: commitment_data_hash, signature};
                            let training_result = Broadcast { step, proof, nonce: thread_rng().next_u32(), commitment, data: BroadcastType::TrainingResult(TrainingResult { batch_id, ticket })};

                            p2p.broadcast(&training_result).await?;
                            broadcasts.push((training_result.clone(), step));

                            // simulate us recving it & apply like anyone else's
                            {
                                run.apply_message(identity,  training_result).await?;

                                // VERY IMPORTANT -- we pass the "original" distro result, which is unquantized
                                // even if quantization is turned on (distro_result is quantized).
                                // this is because distro needs the unquantized version for lookahead
                                run.apply_distro_result(hash, distro_result, Some(original_distro_result)).await;
                            }
                        }

                        _ = sharing_downloadable_interval.tick() => {
                            match broadcasts.len() {
                                0 => {},
                                len => {
                                    // it's possible we've disconnected from a gossip peer, but we don't know until we try and send to them.
                                    // in general, iroh-gossip doesn't guarantee delivery past 99.9%. so, we rebroadcast our live results (-2 rounds)
                                    // periodically
                                    broadcasts_rebroadcast_index = (broadcasts_rebroadcast_index + 1) % len;
                                    let (broadcast, _step) = &mut broadcasts[broadcasts_rebroadcast_index];
                                    broadcast.nonce = broadcast.nonce.wrapping_add(1);
                                    match &broadcast.data {
                                        BroadcastType::TrainingResult(training_result) => trace!(client_id = %identity, step = broadcast.step, nonce = broadcast.nonce, batch_id = %training_result.batch_id, "Rebroadcasting training result"),
                                        BroadcastType::Finished(finished) => trace!(client_id = %identity, step = broadcast.step, nonce = broadcast.nonce, warmup = finished.warmup, "Rebroadcasting finished"),
                                    }
                                    p2p.broadcast(broadcast).await?;
                                }
                            }
                        }

                        _ = retry_check_interval.tick() => {
                            let now = Instant::now();
                            let pending_retries: Vec<(psyche_network::Hash, BlobTicket, u32)> = retried_downloads.iter()
                                .filter(|(_, info)| info.retry_time.map(|retry_time| now >= retry_time).unwrap_or(false) && info.retries <= MAX_DOWNLOAD_RETRIES)
                                .map(|(hash, info)| (*hash, info.ticket.clone(), info.tag))
                                .collect();

                            for (hash, ticket, tag) in pending_retries {
                                if let Some(info) = retried_downloads.get_mut(&hash) {
                                    info.retry_time = None;

                                    debug!("Retrying download for blob {} (attempt {})",
                                        hex::encode(hash), info.retries);
                                    p2p.start_download(ticket, tag).await?;
                                }
                            }
                        }

                        _ = opprotunistic_witness_interval.tick() => {
                            run.try_send_opportunistic_witness().await?;
                        }

                        Some((download_ticket, tag)) = rx_request_download.recv() => {
                            p2p.start_download(download_ticket, tag).await?;
                        }
                        Some(opportunistic_data) = rx_witness.recv() => {
                            watcher.backend_mut().send_witness(opportunistic_data).await?;
                        }
                        Some(health_check) = rx_health_check.recv() => {
                            watcher.backend_mut().send_health_check(health_check).await?;
                        }
                        Some(checkpoint) = rx_checkpoint.recv() => {
                            watcher.backend_mut().send_checkpoint(checkpoint).await?;
                        }
                        Some(model) = rx_model.recv() => {
                            sharable_model.update_parameters(model)?;
                        },
                        Some((config_string, tokenizer_string)) = rx_config.recv() => {
                            let tokenizer: Tokenizer = serde_json::from_str(&tokenizer_string)?;
                            sharable_model.update_config(config_string, tokenizer)?;
                        }
                        Some((param_names, tx_params_response)) = rx_parameters_req.recv() => {
                            sharable_model.initialize_parameters(&param_names, tx_params_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                bail!("Coordinator state not yet registered, nothing to do. Try joining the run again.");
                            };

                            let tx_params_download = tx_params_download.clone();
                            let router = p2p.router();

                            let me = NodeId::from_bytes(identity.get_p2p_public_key()).unwrap();
                            let mut peer_ids: Vec<NodeId> = coordinator_state.epoch_state.clients.iter().map(|client| {
                                let peer_id_bytes = client.id.get_p2p_public_key();
                                NodeId::from_bytes(peer_id_bytes).unwrap()
                            })
                            .filter(|peer_id| peer_id != &me)
                            .collect();

                            if peer_ids.is_empty() {
                                bail!("There are no peers to request parameters from. Try joining the run again.");
                            }
                            peer_ids.shuffle(&mut thread_rng());
                            let num_peers = peer_ids.len();
                            let param_requests_cancel_token = param_requests_cancel_token.clone();
                            let handle: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
                                // We use std mutex implementation here and call `.unwrap()` when acquiring the lock since there
                                // is no chance of mutex poisoning; locks are acquired only to insert or remove items from them
                                // and dropped immediately
                                let parameter_blob_tickets = Arc::new(std::sync::Mutex::new(Vec::new()));
                                let busy_peers = Arc::new(std::sync::Mutex::new(HashSet::new()));
                                let errored_peers = Arc::new(std::sync::Mutex::new(HashMap::new()));

                                let peer_cycle = peer_ids.into_iter().cycle();
                                let peer_cycle = Arc::new(Mutex::new(peer_cycle));
                                let mut request_handles = Vec::new();

                                for param_name in param_names {
                                    let router = router.clone();
                                    let busy_peers = busy_peers.clone();
                                    let peer_cycle = peer_cycle.clone();
                                    let errored_peers = errored_peers.clone();

                                    let request_handle = tokio::spawn(
                                        param_request_task(
                                            ModelRequestType::Parameter(param_name),
                                            router,
                                            parameter_blob_tickets.clone(),
                                            peer_cycle,
                                            busy_peers,
                                            errored_peers,
                                            num_peers,
                                            param_requests_cancel_token.clone()
                                        )
                                    );

                                    // Check if we reached the max number of concurrent requests, and if that is the case,
                                    // await for all of them to complete and start downloading the blobs
                                    if request_handles.len() == max_concurrent_downloads - 1 {
                                        let mut max_concurrent_request_futures = std::mem::take(&mut request_handles);
                                        max_concurrent_request_futures.push(request_handle);
                                        // join_all(max_concurrent_request_futures).await;
                                        join_all(max_concurrent_request_futures).await;
                                        let current_parameter_blob_tickets: Vec<BlobTicket> = {
                                            let mut parameter_blob_tickets_lock = parameter_blob_tickets.lock().unwrap();
                                            parameter_blob_tickets_lock.drain(..).collect()
                                        };
                                        tx_params_download.send(current_parameter_blob_tickets)?;
                                        continue;
                                    }
                                    request_handles.push(request_handle);
                                }

                                // All parameters have been requested, wait all the remaining request futures to complete
                                // and download the blobs
                                join_all(request_handles).await;
                                let parameter_blob_tickets: Vec<BlobTicket> = {
                                    let mut parameter_blob_tickets_lock = parameter_blob_tickets.lock().unwrap();
                                    parameter_blob_tickets_lock.drain(..).collect()
                                };
                                tx_params_download.send(parameter_blob_tickets)?;
                                Ok(())
                            });
                            drop(handle);
                        },
                        Some(tx_model_config_response) = rx_request_model_config.recv() => {
                            sharable_model.tx_model_config_response = Some(tx_model_config_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                warn!("Coordinator state not yet registered, nothing to do");
                                return Ok(());
                            };
                            let router = p2p.router();
                            let me = NodeId::from_bytes(identity.get_p2p_public_key())?;
                            let peer_ids: Vec<NodeId> = coordinator_state.epoch_state.clients.iter().map(|client| {
                                let peer_id_bytes = client.id.get_p2p_public_key();
                                NodeId::from_bytes(peer_id_bytes).unwrap()
                            })
                            .filter(|peer_id| peer_id != &me)
                            .collect();

                            // initialize variables to request model config
                            let parameter_blob_tickets = Arc::new(std::sync::Mutex::new(Vec::new()));
                            let busy_peers = Arc::new(std::sync::Mutex::new(HashSet::new()));
                            let errored_peers = Arc::new(std::sync::Mutex::new(HashMap::new()));
                            let num_peers = peer_ids.len();
                            let peer_cycle = peer_ids.into_iter().cycle();
                            let peer_cycle = Arc::new(Mutex::new(peer_cycle));

                            if num_peers == 0 {
                                return Err(anyhow::anyhow!("No peers available to request the model"))
                            }
                            param_request_task(
                                ModelRequestType::Config,
                                router,
                                parameter_blob_tickets.clone(),
                                peer_cycle,
                                busy_peers,
                                errored_peers,
                                num_peers,
                                param_requests_cancel_token.clone()
                            ).await;

                            let parameter_blob_tickets: Vec<BlobTicket> = {
                                let mut parameter_blob_tickets_lock = parameter_blob_tickets.lock().unwrap();
                                parameter_blob_tickets_lock.drain(..).collect()
                            };

                            for ticket in parameter_blob_tickets {
                                // tag 0 means when we enter a train step, it'll get wiped.
                                p2p.start_download(ticket, 0).await?;
                            }

                        }
                        Some(param_blob_tickets) = rx_params_download.recv() => {
                            for ticket in param_blob_tickets {
                                // tag 0 means when we enter a train step, it'll get wiped.
                                p2p.start_download(ticket, 0).await?;
                            }
                        }
                        _ = param_requests_cancel_token.cancelled() => bail!("Peers were unreachable for P2P parameter requests. Try joining again"),
                        else => break
                    }
                }
                Ok(())
            }
        });

        Self {
            _t: Default::default(),
            cancel,
            req_tui_state,
            rx_tui,
            join,
        }
    }

    pub fn finished(&mut self) -> &mut JoinHandle<Result<(), Error>> {
        &mut self.join
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub async fn tui_states(&self) -> TUIStates {
        self.req_tui_state.notify_one();
        self.rx_tui.borrow().clone()
    }
}

pub struct P2PNodeInfo {
    pub ips: Vec<String>,
    pub bandwidth: f64,
}

async fn get_p2p_info<B, D>(
    p2p: &NetworkConnection<B, D>,
) -> anyhow::Result<HashMap<String, P2PNodeInfo>>
where
    B: Networkable,
    D: Networkable,
{
    let remotes = p2p.remote_infos();
    let node_addr = p2p.node_addr().await?;
    Ok(remotes
        .into_iter()
        .map(|(x, bandwidth)| {
            (
                x.node_id.to_string(),
                P2PNodeInfo {
                    ips: x
                        .addrs
                        .into_iter()
                        .map(|y| y.addr.to_string())
                        .collect::<Vec<_>>(),
                    bandwidth,
                },
            )
        })
        .chain(std::iter::once((
            node_addr.node_id.to_string(),
            P2PNodeInfo {
                ips: node_addr
                    .direct_addresses
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>(),
                bandwidth: 0.0,
            },
        )))
        .collect())
}
