use crate::{
    Broadcast, BroadcastType, ClientTUIState, Finished, NC, RunInitConfig, RunInitConfigAndIO,
    TrainingResult,
    state::{ApplyMessageOutcome, DistroBroadcastAndPayload, FinishedBroadcast, RunManager},
};
use anyhow::anyhow;
use anyhow::{Error, Result, bail};
use psyche_coordinator::{Commitment, CommitteeSelection, Coordinator, RunState};
use psyche_core::{IntegrationTestLogMarker, NodeIdentity};
use psyche_metrics::{ClientMetrics, ClientRoleInRound, PeerConnection};
use psyche_network::{
    AuthenticatableIdentity, DownloadComplete, DownloadSchedulerHandle, DownloadType, EndpointId,
    ModelRequestType, NetworkEvent, NetworkTUIState, PeerManagerHandle, RetryConfig,
    RetryQueueResult, SharableModel, TransmittableDownload, allowlist,
    blob_ticket_param_request_task, raw_p2p_verify,
};
use psyche_watcher::{Backend, BackendWatcher};
use tokenizers::Tokenizer;

use iroh_blobs::api::Tag;
use rand::{Rng, RngCore, seq::SliceRandom};
use std::{
    collections::BTreeSet,
    marker::PhantomData,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    select,
    sync::{Notify, mpsc, watch},
    task::JoinHandle,
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, trace_span, warn};

pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NodeIdentity, A: AuthenticatableIdentity, B: Backend<T> + 'static> {
    rx_tui: watch::Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, A, B)>,
}

const REBROADCAST_SHAREABLE: Duration = Duration::from_secs(10);
const DOWNLOAD_RETRY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const OPPROTUNISTIC_WITNESS_INTERVAL: Duration = Duration::from_millis(500);
const CHECK_CONNECTION_INTERVAL: Duration = Duration::from_secs(10);
const MAX_ERRORS_PER_PEER: u8 = 5;

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static, B: Backend<T> + 'static>
    Client<T, A, B>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: B,
        allowlist: allowlist::AllowDynamic,
        mut p2p: NC,
        init_config: RunInitConfig<T, A>,
        metrics: Arc<ClientMetrics>,
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
                    anyhow::bail!(
                        "Tensor parallelism was set but this build does not support it (must be built with --features=parallelism)"
                    )
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
                let (tx_config_download, mut rx_config_download) = mpsc::unbounded_channel();
                let (tx_request_model_config, mut rx_request_model_config) =
                    mpsc::unbounded_channel();
                let (tx_broadcast_finished, mut rx_broadcast_finished) = mpsc::unbounded_channel();

                let max_concurrent_parameter_requests =
                    init_config.max_concurrent_parameter_requests;

                let mut current_downloaded_parameters = 0_u64;
                let mut total_parameters = None;

                let mut run = RunManager::<T, A>::new(RunInitConfigAndIO {
                    init_config,
                    metrics: metrics.clone(),
                    tx_witness,
                    tx_health_check,
                    tx_checkpoint,
                    tx_model,
                    tx_parameters_req,
                    tx_config,
                    tx_distro_result,
                    tx_request_download: tx_request_download.clone(),
                    tx_request_model_config,
                    tx_broadcast_finished,
                });

                let download_scheduler = DownloadSchedulerHandle::new(
                    max_concurrent_parameter_requests,
                    RetryConfig::default(),
                );
                let mut sharable_model = SharableModel::empty();
                let peer_manager = Arc::new(PeerManagerHandle::new(
                    MAX_ERRORS_PER_PEER,
                    param_requests_cancel_token.clone(),
                    p2p.connection_monitor(),
                ));

                let mut broadcasts = vec![];
                let mut broadcasts_rebroadcast_index = 0;
                let mut sharing_downloadable_interval = interval(REBROADCAST_SHAREABLE);
                let mut retry_check_interval = interval(DOWNLOAD_RETRY_CHECK_INTERVAL);
                let mut opportunistic_witness_interval = interval(OPPROTUNISTIC_WITNESS_INTERVAL);
                let mut check_connection_interval = interval(CHECK_CONNECTION_INTERVAL);
                let mut wait_for_checkpoint = false;
                let mut last_gossip_connection_time = SystemTime::now();
                debug!("Starting client loop");

                loop {
                    select! {
                        _ = cancel.cancelled() => {
                            info!("Got request to cancel main client loop");
                            if run.doing_checkpoint() {
                                wait_for_checkpoint = true;
                            }
                            break;
                        }

                         _ = req_tui_state.notified() => {
                            let network_tui_state = NetworkTUIState::from_network_connection(&p2p).await?;
                            let client_tui_state = (&run).into();
                            tx_tui.send((client_tui_state, network_tui_state))?;
                        },

                        state = watcher.poll_next() => {
                            let (old_state, new_state) = state?;
                            let old_run_state = old_state
                                .map(|s| s.run_state.to_string())
                                .unwrap_or_else(|| String::from(" - "));

                            if old_state.map(|s| s.run_state).unwrap_or_default() != new_state.run_state {
                                info!(
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
                            }

                            let run_participating_endpoint_ids = participating_endpoint_ids(new_state);
                            allowlist.set(run_participating_endpoint_ids);
                            ensure_gossip_connected(new_state, &mut p2p, &mut last_gossip_connection_time);

                            if old_state.map(|s| s.run_state) != Some(new_state.run_state) && new_state.run_state == RunState::RoundTrain {
                                trace!("Updating p2p");
                                let last_needed_step_blobs = new_state.progress.step.saturating_sub(2);
                                if let Err(err) = p2p.remove_staled_tags(last_needed_step_blobs).await {
                                    warn!("Error deleting blob tags less than {last_needed_step_blobs}: {err}");
                                }
                                let p2p_info = p2p.remote_infos();
                                metrics.update_bandwidth(p2p_info.iter().map(|v| v.bandwidth).sum());
                                if let Err(e) = run.set_endpoint_info(p2p_info) {
                                    warn!("failed to set p2p info: {e}");
                                }
                                broadcasts.retain(|(_, step)| *step >= last_needed_step_blobs);
                                sharable_model.clear_cache(); // IMPORTANT -- any cached blobs are now invalid
                            }

                            run.apply_state(*new_state).await?;
                            {
                                let current_step = run.coordinator_state().map(|s| s.progress.step).unwrap_or(0);
                                let role = {
                                    let client_index = new_state
                                        .epoch_state
                                        .clients
                                        .iter()
                                        .position(|x| x.id == identity);
                                    let round = new_state.current_round();
                                    let committee_selection = round.and_then(|round|
                                        CommitteeSelection::new(
                                            round.tie_breaker_tasks as usize,
                                            new_state.config.witness_nodes as usize,
                                            new_state.config.verification_percent,
                                            new_state.epoch_state.clients.len(),
                                            round.random_seed,
                                        ).ok()
                                    );
                                    match (client_index, committee_selection) {
                                        (Some(i), Some(s)) => if s.get_witness(i as u64).witness.into() {
                                            ClientRoleInRound::Witness
                                        } else {
                                            ClientRoleInRound::Trainer
                                        }
                                        _ => ClientRoleInRound::NotInRound,
                                    }
                                };
                                metrics.update_round_state(current_step, role);
                            }
                        }

                        res = p2p.poll_next() => {
                            if let Some(message) = res? {
                                match message {
                                    NetworkEvent::MessageReceived((from, broadcast)) => {
                                        let _ = trace_span!("NetworkEvent::MessageReceived", from=%from).entered();
                                        metrics.record_broadcast_seen();
                                        let broadcast_step = broadcast.step;
                                        let broadcast_kind = broadcast.data.kind();
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
                                                let apply_result = run.apply_message(client.id, broadcast)?;
                                                match apply_result {
                                                    ApplyMessageOutcome::Ignored => {
                                                        metrics.record_apply_message_ignored(broadcast_kind);
                                                    },
                                                    ApplyMessageOutcome::Applied => {
                                                        metrics.record_apply_message_success(broadcast_step, from, broadcast_kind);
                                                    },
                                                    ApplyMessageOutcome::Invalid => {
                                                        metrics.record_apply_message_failure(broadcast_step, from, broadcast_kind);
                                                    }
                                                }
                                            } else {
                                                warn!(from=from.fmt_short().to_string(), "Invalid signature on commitment from {}", from.fmt_short());
                                                metrics.record_apply_message_failure(broadcast_step, from, broadcast_kind);
                                            }
                                        } else {
                                            trace!("Got broadcast from unknown client {}", from);
                                            metrics.record_apply_message_failure(broadcast_step, from, broadcast_kind);
                                        }
                                    }
                                    NetworkEvent::DownloadComplete(DownloadComplete {
                                        data: download_data, hash, from
                                    }) => {
                                        let _ = trace_span!("NetworkEvent::DownloadComplete", hash = %hash).entered();
                                        metrics.record_download_completed(hash, from);
                                        // Remove from retry queue if it was a retry
                                        if download_scheduler.remove_retry(hash).await {
                                            info!("Successful download after retry for blob hash 0x{}", hex::encode(hash));
                                        };
                                        match download_data {
                                            TransmittableDownload::DistroResult(distro_result) => {
                                                debug!("Download complete: step {} batch id {}", distro_result.step, distro_result.batch_id);
                                                run.apply_distro_result(hash, distro_result, None);
                                            },
                                            TransmittableDownload::ModelParameter(parameter) => {
                                                // Release capacity for parameter downloads
                                                download_scheduler.release_capacity();
                                                current_downloaded_parameters += 1;
                                                info!("Download complete: parameter {}", parameter.name()?);
                                                if let Some(total_parameters) = total_parameters {
                                                    info!("Downloaded parameters total: {}/{}", current_downloaded_parameters, total_parameters);
                                                    metrics.update_model_sharing_total_params_downloaded(current_downloaded_parameters);
                                                } else {
                                                    error!("Total parameters not set");
                                                }
                                                sharable_model.add_parameter(parameter).await?;
                                                if sharable_model.is_download_complete() {
                                                    sharable_model.send_init_parameters()?;
                                                }
                                            },
                                            TransmittableDownload::ModelConfig(config) => {
                                                info!("Download complete: model config");
                                                sharable_model.add_config(config)?;
                                                sharable_model.send_config()?;
                                            },
                                        }
                                    }
                                    NetworkEvent::DownloadFailed(dl) => {
                                        let _ = trace_span!("NetworkEvent::DownloadFailed", error=%dl.error).entered();
                                        let hash = dl.blob_ticket.hash();

                                        match dl.download_type {
                                            DownloadType::ModelSharing(request_type) => {
                                                // Only release capacity for parameter downloads;
                                                // config downloads don't consume capacity
                                                if matches!(request_type, ModelRequestType::Parameter(_)) {
                                                    download_scheduler.release_capacity();
                                                }

                                                metrics.record_p2p_model_parameter_download_failed();
                                                peer_manager.report_blob_ticket_request_error(dl.blob_ticket.addr().id, Some(dl.blob_ticket.clone()));

                                                info!(
                                                    "Model Sharing download failed with provider node {} (will retry): {}",
                                                    dl.blob_ticket.addr().id,
                                                    dl.error
                                                );

                                                let download_type = DownloadType::ModelSharing(request_type.clone());
                                                let router = p2p.router().clone();
                                                let peer_manager = peer_manager.clone();
                                                let download_scheduler = download_scheduler.clone();
                                                let param_requests_cancel_token = param_requests_cancel_token.clone();
                                                tokio::spawn(async move {
                                                    let blob_ticket_to_retry = if let Ok((new_blob_ticket, _)) = blob_ticket_param_request_task(request_type, router.clone(), peer_manager.clone(), param_requests_cancel_token).await {
                                                        // We remove the old hash because we're getting the blob from a new peer that has its own version of the model parameter or config blob
                                                        download_scheduler.remove_retry(hash).await;
                                                        new_blob_ticket
                                                    } else {
                                                        dl.blob_ticket
                                                    };

                                                    download_scheduler.queue_failed_download(
                                                        blob_ticket_to_retry,
                                                        dl.tag,
                                                        download_type,
                                                    ).await;
                                                });
                                            }
                                            DownloadType::DistroResult(_) => {
                                                let result = download_scheduler.queue_failed_download(
                                                    dl.blob_ticket,
                                                    dl.tag,
                                                    dl.download_type,
                                                ).await;

                                                match result {
                                                    RetryQueueResult::Queued => {
                                                        metrics.record_download_failed();
                                                        info!(
                                                            "Distro result download failed (will retry with backoff): {}",
                                                            dl.error
                                                        );
                                                    }
                                                    RetryQueueResult::MaxRetriesExceeded => {
                                                        metrics.record_download_perma_failed();
                                                        warn!("Distro result download failed (not retrying): {}", dl.error);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx) => {
                                        // TODO: We should validate that the parameter is requested while we are in RunState::Warmup.
                                        trace!("NetworkEvent::ParameterRequest({parameter_name})");
                                        match sharable_model.get_transmittable_parameter(&parameter_name, &mut p2p, Tag::from(format!("model-{parameter_name}"))).await {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(ticket) => {
                                                info!(parameter = parameter_name, hash = %ticket.hash(), "Sending requested model parameter blob ticket");
                                                if let Err(e) = protocol_req_tx.send(Ok(ticket)) {
                                                    warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                                };
                                            }
                                        }
                                    },
                                    NetworkEvent::ModelConfigRequest(protocol_req_tx) => {
                                        trace!("NetworkEvent::ModelConfigRequest");
                                        match sharable_model.get_transmittable_config(&mut p2p, "model-config").await {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model config blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(config_ticket) => {
                                                info!(hash = %config_ticket.hash(), "Sending requested model config blob ticket");
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
                            let training_result = Broadcast { step, proof, nonce: rand::rng().next_u32(), commitment, data: BroadcastType::Finished(Finished {
                                broadcast_merkle: merkle, warmup
                            })};

                            p2p.broadcast(&training_result)?;
                            broadcasts.push((training_result.clone(), step));

                            // simulate us recving it & apply like anyone else's
                            run.apply_message(identity,  training_result)?;
                        }

                        Some(DistroBroadcastAndPayload { step, batch_id, commitment_data_hash, proof, distro_result, original_distro_result }) = rx_distro_result.recv() => {

                            let transmittable_distro_result = TransmittableDownload::DistroResult(distro_result.clone());

                            let tag_name = format!("distro-result_{step}");
                            let (ticket, size) = p2p.add_downloadable(transmittable_distro_result, Tag::from(tag_name)).await?;

                            let hash = ticket.hash();
                            info!(
                                client_id = %identity, step = step,
                                "Broadcasting payload batch id {batch_id} hash 0x{} ({:.3} MB)",
                                hex::encode(hash),
                                (size as f64 ) / 1_000_000f64
                            );

                            let signature = network_identity.raw_p2p_sign(&private_key, &commitment_data_hash);
                            let commitment = Commitment { data_hash: commitment_data_hash, signature};
                            let training_result = Broadcast { step, proof, nonce: rand::rng().random(), commitment, data: BroadcastType::TrainingResult(TrainingResult { batch_id, ticket })};

                            p2p.broadcast(&training_result)?;
                            broadcasts.push((training_result.clone(), step));

                            // simulate us recving it & apply like anyone else's
                            run.apply_message(identity, training_result)?;

                            // VERY IMPORTANT -- we pass the "original" distro result, which is unquantized
                            // even if quantization is turned on (distro_result is quantized).
                            // this is because distro needs the unquantized version for lookahead
                            run.apply_distro_result(hash, distro_result, Some(original_distro_result));
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
                                    p2p.broadcast(broadcast)?;
                                }
                            }
                        }

                        _ = retry_check_interval.tick() => {
                            // Handle DistroResult retries (no rate limiting)
                            for retry in download_scheduler.get_due_distro_retries().await {
                                metrics.record_download_retry(retry.hash);
                                info!("Retrying download for distro result, (attempt {})", retry.retries);
                                let _ = tx_request_download.send((retry.ticket, retry.tag));
                            }

                            // Handle config retries (no capacity limiting, config doesn't consume slots)
                            for retry in download_scheduler.get_due_config_retries().await {
                                metrics.record_download_retry(retry.hash);
                                info!("Retrying download for model config, (attempt {})", retry.retries);
                                let _ = tx_config_download.send(retry.ticket);
                            }

                            // Handle parameter retries (with rate limiting via scheduler capacity)
                            while let Some(retry) = download_scheduler.start_parameter_retry().await {
                                metrics.record_download_retry(retry.hash);
                                if let DownloadType::ModelSharing(ModelRequestType::Parameter(ref parameter)) = retry.download_type {
                                    info!("Retrying download for model parameter: {parameter}, (attempt {})", retry.retries);
                                    if tx_params_download.send((retry.ticket, ModelRequestType::Parameter(parameter.clone()))).is_err() {
                                        warn!("Failed to send parameter retry for {parameter}, releasing capacity");
                                        download_scheduler.release_capacity();
                                    }
                                } else {
                                    // Unexpected download type from start_parameter_retry, release the capacity slot
                                    download_scheduler.release_capacity();
                                }
                            }
                        }

                        _ = opportunistic_witness_interval.tick() => {
                            run.try_send_opportunistic_witness().await?;
                        }

                        Some((download_ticket, tag)) = rx_request_download.recv() => {
                            let self_endpoint_id = p2p.endpoint_id();
                            let other_possible_nodes = run.coordinator_state().map(all_endpoint_ids_shuffled).unwrap_or_default();
                            let other_possible_nodes = other_possible_nodes.into_iter().filter(|addr| *addr != self_endpoint_id).collect();
                            let kind = DownloadType::DistroResult(other_possible_nodes);
                            metrics.record_download_started(download_ticket.hash(), kind.kind());
                            p2p.start_download(download_ticket, tag, kind);
                        }
                        Some(opportunistic_data) = rx_witness.recv() => {
                            metrics.record_witness_send(opportunistic_data.kind());
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
                            metrics.initialize_model_parameters_gauge(param_names.len().try_into().unwrap());
                            total_parameters = Some(param_names.len());
                            sharable_model.initialize_parameters(&param_names, tx_params_response);

                            let router = p2p.router();

                            let peer_manager = peer_manager.clone();
                            let param_requests_cancel_token = param_requests_cancel_token.clone();
                            let download_scheduler = download_scheduler.clone();
                            let tx_params_download = tx_params_download.clone();

                            tokio::spawn(async move {
                                for param_name in param_names {
                                    if let Err(e) = download_scheduler.wait_for_capacity().await {
                                        error!("Download scheduler shut down, aborting parameter requests: {e}");
                                        break;
                                    }

                                    let router = router.clone();

                                    match blob_ticket_param_request_task(
                                        ModelRequestType::Parameter(param_name.clone()),
                                        router,
                                        peer_manager.clone(),
                                        param_requests_cancel_token.clone()
                                    ).await {
                                        Ok((blob_ticket, request_type)) => {
                                            // Send the download request
                                            if tx_params_download.send((blob_ticket, request_type)).is_err() {
                                                error!("Failed to send parameter download request for {}", param_name);
                                                // Release capacity if send failed
                                                download_scheduler.release_capacity();
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to get blob ticket for parameter {}: {}", param_name, e);
                                            // Release capacity since we didn't start a download
                                            download_scheduler.release_capacity();
                                        }
                                    }
                                }
                            });
                        },
                        Some(tx_model_config_response) = rx_request_model_config.recv() => {
                            sharable_model.tx_model_config_response = Some(tx_model_config_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                warn!("Coordinator state not yet registered, nothing to do");
                                return Ok(());
                            };

                            let me = EndpointId::from_bytes(identity.get_p2p_public_key())?;
                            let peer_ids: Vec<EndpointId> = participating_endpoint_ids(&coordinator_state)
                                .into_iter()
                                .filter(|peer_id| peer_id != &me)
                                .collect();

                            let peer_manager = peer_manager.clone();
                            peer_manager.set_peers(peer_ids);
                            let router = p2p.router().clone();
                            let tx_config_download = tx_config_download.clone();
                            let param_requests_cancel_token = param_requests_cancel_token.clone();
                            tokio::spawn(async move {
                                if let Ok((config_blob_ticket, _)) = blob_ticket_param_request_task(ModelRequestType::Config, router.clone(), peer_manager, param_requests_cancel_token).await {
                                    tx_config_download.send(config_blob_ticket).expect("Failed to send config blob ticket");
                                } else {
                                    error!("Error getting the config blob ticket, we'll not proceed with the download");
                                }
                            });
                        }
                        Some(param_blob_tickets) = rx_params_download.recv() => {
                            let (ticket, request_type) = param_blob_tickets;
                            let kind = DownloadType::ModelSharing(request_type.clone());
                            metrics.record_download_started(ticket.hash(), kind.kind());
                            if let ModelRequestType::Parameter(parameter_name) = request_type {
                                p2p.start_download(ticket, Tag::from(format!("model-{parameter_name}")), kind);
                            }
                        }
                        Some(config_blob_ticket) = rx_config_download.recv() => {
                            let kind = DownloadType::ModelSharing(ModelRequestType::Config);
                            metrics.record_download_started(config_blob_ticket.hash(), kind.kind());
                            p2p.start_download(config_blob_ticket, Tag::from("model-config"), kind);
                        }
                        _ = param_requests_cancel_token.cancelled() => bail!("Peers were unreachable for P2P parameter requests. Try joining again"),
                        _ = check_connection_interval.tick() => {
                            let Some(run_state) = run.coordinator_state() else {continue;};
                            if run_state.halted() {
                                continue;
                            }

                            {
                                let remote_infos: Vec<_> = p2p
                                    .remote_infos()
                                    .into_iter()
                                    .filter(|info| info.selected_path.is_some())
                                    .map(|info| {
                                        PeerConnection {
                                            endpoint_id: info.id.to_string(),
                                            selected_path: info.selected_path,
                                        }
                                    })
                                    .collect();
                                metrics.update_peer_connections(&remote_infos);
                            }
                            ensure_gossip_connected(run_state, &mut p2p, &mut last_gossip_connection_time);
                        }
                        else => break
                    }
                }

                info!("Main client loop ended");

                let p2p_shutdown = p2p.shutdown();

                if wait_for_checkpoint {
                    info!("Waiting for all pending checkpoints to finish");

                    // Keep waiting for checkpoints while there are uploads pending
                    let mut checkpoint_check_interval = interval(Duration::from_secs(10));
                    while run.doing_checkpoint() {
                        tokio::select! {
                            checkpoint = rx_checkpoint.recv() => {
                                if let Some(checkpoint) = checkpoint {
                                    info!("Checkpoint upload completed, sending to Solana");
                                    watcher.backend_mut().send_checkpoint(checkpoint).await?;
                                } else {
                                    // Channel closed, no more checkpoints coming
                                    break;
                                }
                            }
                            _ = checkpoint_check_interval.tick() => {
                            }
                        }
                    }

                    info!("All checkpoints finished, exiting main client loop");
                }

                p2p_shutdown
                    .await
                    .map_err(|e| anyhow!("Error shutting down p2p: {e}"))
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

fn ensure_gossip_connected<T: NodeIdentity>(
    run_state: &Coordinator<T>,
    p2p: &mut NC,
    last_connection_attempt: &mut SystemTime,
) {
    // don't try to connect to anyone if we're paused
    if run_state.halted() {
        return;
    }

    // if we tried too recently, don't bother.
    if SystemTime::now()
        .duration_since(*last_connection_attempt)
        .unwrap_or(Duration::ZERO)
        < Duration::from_secs(6)
    {
        return;
    }

    let my_endpoint_id = p2p.endpoint_id();

    let run_participating_endpoint_ids = participating_endpoint_ids(run_state);

    // only connect to peers after we become part of the set of current clients
    if !run_participating_endpoint_ids.contains(&my_endpoint_id) {
        return;
    }

    *last_connection_attempt = SystemTime::now();

    // TODO: maybe don't force connections if we're trying to join new peers already
    // see https://github.com/PsycheFoundation/psyche/issues/78
    let gossip_neighbors: BTreeSet<_> = p2p.neighbors().collect();
    if gossip_neighbors.is_empty() {
        warn!("Not connected to any gossip peers! Trying to connect to some...");
    }

    const MAX_NUM_BOOTSTRAP_PEERS: usize = 3;
    // we only want to bootstrap gossip;
    // only connect to enough peers to bring our total peer count to at MOST MAX_NUM_BOOTSTRAP_PEERS.
    // if we already have that many or more, don't send any gossip joins
    // because gossip joins this way can force-disconnect other peers.
    let num_peers_to_add = MAX_NUM_BOOTSTRAP_PEERS.saturating_sub(gossip_neighbors.len());

    if num_peers_to_add == 0 {
        return;
    }

    let mut to_connect = run_participating_endpoint_ids
        .iter()
        .filter(|id| *id != &my_endpoint_id)
        .filter(|id| !gossip_neighbors.contains(*id))
        .collect::<Vec<_>>();
    to_connect.shuffle(&mut rand::rng());
    let to_connect = to_connect
        .into_iter()
        .take(num_peers_to_add)
        .cloned()
        .collect::<Vec<_>>();

    if !to_connect.is_empty() {
        info!(num_new_peers = to_connect.len(), "Connecting to new peers");
        p2p.add_peers(to_connect);
    }
}

fn participating_endpoint_ids<T: NodeIdentity>(state: &Coordinator<T>) -> Vec<EndpointId> {
    state
        .epoch_state
        .clients
        .iter()
        .map(|c| EndpointId::from_bytes(c.id.get_p2p_public_key()).unwrap())
        .collect()
}

fn all_endpoint_ids_shuffled<T: NodeIdentity>(state: &Coordinator<T>) -> Vec<EndpointId> {
    let mut addrs = participating_endpoint_ids(state);
    addrs.shuffle(&mut rand::rng());
    addrs
}
