use crate::{
    fetch_data::{BatchIdSet, DataFetcher, TrainingDataForStep},
    state::types::{DeserializeError, PayloadState},
};

use futures::{StreamExt, future::try_join_all, stream::FuturesUnordered};
use psyche_coordinator::{
    BLOOM_FALSE_RATE, Commitment, CommitteeSelection, Coordinator, CoordinatorError, HealthChecks,
    assign_data_for_state, get_batch_ids_for_node, get_batch_ids_for_round, model,
};
use psyche_core::{BatchId, Bloom, IntegrationTestLogMarker, NodeIdentity, OptimizerDefinition};
use psyche_modeling::{
    ApplyDistroResultError, Batch, BatchData, DistroResult, TrainOutput, Trainer,
    TrainerThreadCommunicationError,
};
use psyche_network::{
    AuthenticatableIdentity, Hash, SerializeDistroResultError, SerializedDistroResult,
    TransmittableDistroResult, distro_results_to_bytes,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info, trace, trace_span, warn};

use super::{
    evals::{MaybeRunningEvals, ModelTaskRunner},
    round_state::RoundState,
    types::DistroBroadcastAndPayload,
};

#[derive(Debug)]
pub struct FinishedTrainers {
    pub evals_or_trainers: MaybeRunningEvals,
    pub round_losses: Vec<f32>,
    pub optim_stats: HashMap<String, f64>,
    pub round_duration: Duration,
}

#[derive(Error, Debug)]
pub enum TrainError {
    #[error("No trainers available when entering training step.")]
    NoTrainers,

    #[error("No training round in-progress")]
    NoActiveRound,

    #[error("No committee info for this round ")]
    NoCommitteeInfo,

    #[error("We're not in this round")]
    NotInThisRound,

    #[error("Apply thread crashed")]
    ApplyCrashed,

    #[error("Failed to apply distro results: {0}")]
    Apply(#[from] ApplyError),

    #[error("Training thread crashed")]
    TrainCrashed,

    #[error("Transmit thread crashed")]
    TransmitCrashed,

    #[error("Failed to train on batch: {0}")]
    TrainOnBatch(#[from] TrainerThreadCommunicationError),

    #[error("Failed to serialize distro result: {0}")]
    SerializeDistroResult(SerializeDistroResultError),

    #[error("Failed to send distro result, channel must be closed")]
    SendDistroResult,

    #[error("Failed to send health checks, channel must be closed")]
    SendHealthChecks,

    #[error("Health check thread crashed")]
    HealthCheckCrashed,

    #[error("Coordinator error: {0}")]
    CoordinatorError(CoordinatorError),
}

pub struct TrainingStepMetadata<T: NodeIdentity, A: AuthenticatableIdentity> {
    pub identity: T,
    pub data_fetcher: DataFetcher<T, A>,
    pub tx_health_check: mpsc::UnboundedSender<HealthChecks<T>>,
    pub tx_distro_result: mpsc::UnboundedSender<DistroBroadcastAndPayload>,

    pub write_gradients_dir: Option<PathBuf>,

    pub model_task_runner: ModelTaskRunner,
}

#[derive(Debug)]
pub struct TrainingStep {
    sending_health_checks: Option<JoinHandle<Result<(), TrainError>>>,
    cancel_training: CancellationToken,

    applying_and_training: JoinHandle<Result<FinishedTrainers, TrainError>>,
    finished: Arc<AtomicBool>,
}

impl TrainingStep {
    pub async fn finish(self) -> Result<FinishedTrainers, TrainError> {
        self.cancel_training.cancel();
        if let Some(hc) = self.sending_health_checks {
            hc.await.map_err(|_| TrainError::HealthCheckCrashed)??;
        }

        let finished = self.finished.clone();

        let trainers: FinishedTrainers = self
            .applying_and_training
            .await
            .map_err(|_| TrainError::TrainCrashed)??;

        if !finished.load(Ordering::SeqCst) {
            warn!("Training didn't finish when the Training round ended, we are likely to desync.");
        }

        Ok(trainers)
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> TrainingStepMetadata<T, A> {
    pub fn start(
        &mut self,
        client_index: u64,
        state: &Coordinator<T>,
        trainers: Vec<Trainer>,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> Result<TrainingStep, TrainError> {
        if trainers.is_empty() {
            return Err(TrainError::NoTrainers);
        }

        let applying = self.apply_results(trainers, state, previous_round, current_round)?;

        let sending_health_checks =
            start_sending_health_checks(current_round, state, self.tx_health_check.clone())?;

        debug!("Transitioning to train step {}", state.progress.step);

        let cancel_training = CancellationToken::new();
        let round_start = Instant::now();

        let round = state.current_round().ok_or(TrainError::NoActiveRound)?;

        *previous_round = std::mem::take(current_round);

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.config.witness_nodes as usize,
            state.config.verification_percent,
            state.epoch_state.clients.len(),
            round.random_seed,
        )
        .map_err(TrainError::CoordinatorError)?;

        let have_training = !state.epoch_state.last_step_set();
        let (data_assignments, num_all_batch_ids, batch_ids_not_yet_trained_on) = if have_training {
            let data_assignments = assign_data_for_state(state, &committee_selection);
            let all_batch_ids = get_batch_ids_for_round(
                state.current_round().unwrap(),
                state,
                committee_selection.get_num_trainer_nodes(),
            );
            let num_all_batch_ids = all_batch_ids.len();
            let batch_ids_not_yet_trained_on: BatchIdSet = all_batch_ids.into_iter().collect();
            (
                data_assignments,
                num_all_batch_ids,
                Arc::new(Mutex::new(Some(batch_ids_not_yet_trained_on))),
            )
        } else {
            (BTreeMap::new(), 0, Arc::new(Mutex::new(None)))
        };

        let committee_proof = committee_selection.get_committee(client_index);
        let witness_proof = committee_selection.get_witness(client_index);

        let blooms = {
            let participant_bloom =
                Bloom::random(state.epoch_state.clients.len(), BLOOM_FALSE_RATE);
            let broadcast_bloom = Bloom::random(num_all_batch_ids, BLOOM_FALSE_RATE);
            trace!(
                "Participant bloom size: {} bits, {} keys",
                participant_bloom.bits.0.len(),
                participant_bloom.keys.len()
            );
            trace!(
                "Broadcast bloom size: {} bits, {} keys",
                broadcast_bloom.bits.0.len(),
                broadcast_bloom.keys.len()
            );
            Arc::new(Mutex::new(Some((participant_bloom, broadcast_bloom))))
        };

        *current_round = RoundState {
            height: round.height,
            step: state.progress.step,
            sent_witness: false,
            sent_finished: false,
            downloads: Default::default(),
            results: Default::default(),
            broadcasts: Default::default(),
            clients_finished: Default::default(),
            data_assignments: data_assignments.clone(),
            blooms,
            committee_info: Some((committee_proof, witness_proof, committee_selection)),
            batch_ids_not_yet_trained_on,
            self_distro_results: vec![],
        };

        let warmup_lr_between = state.get_cold_start_warmup_bounds();
        let zero_optim = warmup_lr_between.is_some_and(|_| round.height == 0);
        let epoch = state.progress.epoch;

        info!(
            integration_test_log_marker = %IntegrationTestLogMarker::WitnessElected,
            step = state.progress.step,
            round = round.height,
            epoch = epoch,
            index = client_index,
            committee_position = committee_proof.position,
            committee = %committee_proof.committee,
            witness_position = witness_proof.position,
            witness = %witness_proof.witness,
            warmup_lr_between = ?warmup_lr_between,
            assigned_batches = ?get_batch_ids_for_node(&data_assignments, &self.identity),
            "Got training assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={} warmup_lr_between={:?}",
            state.progress.step, round.height, epoch, client_index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness, warmup_lr_between
        );
        let model_task_runner = self.model_task_runner.clone();
        let finished = Arc::new(AtomicBool::new(false));

        let prev_self_distro_results = previous_round.self_distro_results.clone();
        let applying_and_training: JoinHandle<Result<FinishedTrainers, TrainError>> =
            if !have_training {
                let finished = finished.clone();

                // the last two rounds have no training (just applying the final results)
                tokio::task::spawn(async move {
                    let round_duration = Instant::now() - round_start;
                    debug!("Training for round finished, duration {:?}", round_duration);
                    finished.store(true, Ordering::SeqCst);
                    Ok(FinishedTrainers {
                        evals_or_trainers: MaybeRunningEvals::Running(
                            model_task_runner
                                .start(applying.await.map_err(|_| TrainError::ApplyCrashed)??),
                        ),
                        round_losses: vec![],
                        optim_stats: HashMap::new(),
                        round_duration,
                    })
                })
            } else {
                let identity = self.identity;
                let cancel_training = cancel_training.clone();
                let write_gradients_dir = self.write_gradients_dir.clone();
                let tx_distro_result = self.tx_distro_result.clone();
                let quantize = match &state.model {
                    model::Model::LLM(llm) => match llm.optimizer {
                        OptimizerDefinition::Distro { quantize_1bit, .. } => quantize_1bit,
                        _ => false,
                    },
                };
                let finished = finished.clone();

                let TrainingDataForStep {
                    step,
                    mut next_sample,
                } = self
                    .data_fetcher
                    .fetch_data(state, &data_assignments, &self.identity);

                tokio::task::spawn(async move {
                    let mut round_losses: Vec<f32> = Vec::new();
                    let mut optim_stats: HashMap<String, f64> = HashMap::new();

                    let mut available_trainers =
                        applying.await.map_err(|_| TrainError::ApplyCrashed)??;

                    while let Some(data) = next_sample.recv().await {
                        let mut in_progress = FuturesUnordered::new();

                        // reset the DP barriers
                        if let Some(trainer) = available_trainers.first() {
                            #[allow(irrefutable_let_patterns)]
                            if let Trainer::Local(trainer) = trainer {
                                if available_trainers.len() != trainer.data_parallel_world_size() {
                                    error!("Available trainers does not equal DP world size");
                                    return Err(TrainError::TrainCrashed);
                                }
                                trainer.data_parallel_barrier();
                            }
                        } else {
                            error!("No available trainers");
                            return Err(TrainError::TrainCrashed);
                        }

                        let batches = match &data.data {
                            BatchData::CPU(items) => {
                                let total_size = items.len();
                                let num_trainers = available_trainers.len();
                                let chunk_size = total_size / num_trainers;
                                let mut batches = items
                                    .chunks(chunk_size)
                                    .map(|x| x.to_owned())
                                    .collect::<Vec<_>>();
                                if batches.len() == num_trainers + 1 {
                                    let last = batches.pop().unwrap();
                                    for (i, sample) in last.into_iter().enumerate() {
                                        batches[i].push(sample);
                                    }
                                }
                                if batches.len() != num_trainers {
                                    error!("Batches does not match DP world size");
                                }
                                batches
                            }
                            BatchData::GPU(_) => {
                                error!("Got data on GPU before distribution to trainers");
                                return Err(TrainError::TrainCrashed);
                            }
                        };

                        for (trainer, batch_data) in available_trainers.drain(..).zip(batches) {
                            let batch_id = data.id;
                            let batch_data = batch_data.to_vec();
                            let cancel_training = cancel_training.clone();
                            let prev_self_distro_results = prev_self_distro_results.clone();
                            in_progress.push(tokio::task::spawn_blocking(move || {
                                trainer.train(
                                    step,
                                    Batch {
                                        id: batch_id,
                                        data: BatchData::CPU(batch_data),
                                    },
                                    warmup_lr_between,
                                    zero_optim,
                                    Vec::new(),
                                    Some(prev_self_distro_results),
                                    cancel_training,
                                )
                            }));
                        }

                        // the distro results are identical across all ranks, so we just send the first one we get
                        let mut sent_results = false;

                        while let Some(completed_trainer) = in_progress.next().await {
                            let TrainOutput {
                                batch_id,
                                trainer,
                                loss,
                                step,
                                distro_results,
                                cancelled,
                                nonce,
                            } = completed_trainer.map_err(|_| TrainError::TrainCrashed)??;

                            debug!(step=step, loss=loss, batch_id=%batch_id, "Got training output, DisTrO results generated");

                            available_trainers.push(trainer);

                            if !sent_results {
                                let distro_results = distro_results.unwrap_or_default();

                                for result in &distro_results {
                                    if let Some(stats) = &result.stats {
                                        for (name, value) in stats {
                                            // a rolling average for this step :)
                                            optim_stats
                                                .entry(name.clone())
                                                .and_modify(|e| *e = (*e + value) / 2.0)
                                                .or_insert(*value);
                                        }
                                    }
                                }
                                let write_gradients_dir = write_gradients_dir.clone();
                                let tx_distro_result = tx_distro_result.clone();
                                let res: Result<(), TrainError> = tokio::task::spawn_blocking(move || {
                                    if cancelled {
                                        trace!("However, we were cancelled, so we're throwing away this result.");
                                        // we're throwing away this result.
                                        return Ok(());
                                    }

                                    let to_transmit = if quantize { Trainer::quantize_results(&distro_results) } else { distro_results.clone()};

                                    let transmittable_distro_result = TransmittableDistroResult {
                                        step,
                                        batch_id,
                                        distro_results: to_transmit
                                            .into_iter()
                                            .map(|x| SerializedDistroResult::try_from(&x))
                                            .collect::<std::result::Result<Vec<_>, _>>()
                                            .map_err(TrainError::SerializeDistroResult)?,
                                        trainer_nonce: nonce,
                                    };

                                    if let Some(dir) = write_gradients_dir {
                                        let transmittable_distro_result = transmittable_distro_result.clone();
                                        let dir = dir.clone();
                                        tokio::spawn(async move {
                                            if let Err(err) =
                                                write_gradients_to_disk(dir, identity, transmittable_distro_result).await
                                            {
                                                error!("Failed to write gradients to disk: {err:#}");
                                            }
                                        });
                                    }

                                    let commitment_data_hash = transmittable_distro_result.comptue_hash();

                                    trace!("trying to queue tx distro result...");
                                    tx_distro_result
                                        .send(DistroBroadcastAndPayload {
                                            step,
                                            batch_id,
                                            commitment_data_hash,
                                            proof: committee_proof,
                                            distro_result: transmittable_distro_result,
                                            original_distro_result: distro_results,
                                        })
                                        .map_err(|_| TrainError::SendDistroResult)?;
                                    trace!("successfully queued tx distro result");
                                    Ok(())
                                }).await.map_err(|_| TrainError::TransmitCrashed)?;
                                res?;

                                round_losses.push(loss);
                                sent_results = true;
                            }
                        }
                    }

                    let evals = if cancel_training.is_cancelled() {
                        // we got timed out, don't bother starting evals
                        MaybeRunningEvals::NotRunning(available_trainers)
                    } else {
                        // we finished before getting cancelled, have some time to start evals.
                        MaybeRunningEvals::Running(model_task_runner.start(available_trainers))
                    };
                    let round_duration = Instant::now() - round_start;
                    debug!("Training for round finished, duration {:?}", round_duration);
                    finished.store(true, Ordering::SeqCst);
                    Ok(FinishedTrainers {
                        evals_or_trainers: evals,
                        round_losses,
                        optim_stats,
                        round_duration,
                    })
                })
            };

        Ok(TrainingStep {
            applying_and_training,
            cancel_training,
            sending_health_checks,
            finished,
        })
    }

    fn apply_results(
        &mut self,
        trainers: Vec<Trainer>,
        state: &Coordinator<T>,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> Result<JoinHandle<Result<Vec<Trainer>, ApplyError>>, ApplyError> {
        if current_round.height == 0 {
            // the first TWO training step of each epoch has no apply phase.
            // but, because we call this once with the default initalized RoundState (round 0)
            // and a second time (when transitioning from round 0 -> round 1), this check will skip
            // the two phases
            trace!("Skipping early apply");
            return Ok(tokio::task::spawn(async move { Ok(trainers) }));
        }

        let apply_start = Instant::now();
        let step = state.progress.step;
        let witness_quorum = state.witness_quorum(
            state
                .previous_round()
                .ok_or(ApplyError::NoActiveRound)?
                .witnesses
                .len() as u16,
        );
        let (cold_start_warmup_steps, checkpoint_is_p2p) = match &state.model {
            model::Model::LLM(llm) => (
                llm.cold_start_warmup_steps,
                matches!(
                    llm.checkpoint,
                    model::Checkpoint::P2P(_) | model::Checkpoint::P2PGcs(_)
                ),
            ),
        };
        let warmup_lr_between = state.get_cold_start_warmup_bounds();

        // coordinator has already advanced to the next round (unless we're in cooldown) but we haven't started ours yet.
        // so our current_round corresponds to the coordinator's previous_round
        // `previous_round` -> state.previous_previous_round()
        // `current_round` -> state.previous_round()
        let payloads = std::mem::take(&mut previous_round.downloads);
        let commitments = std::mem::take(&mut previous_round.results);

        // here, when dealing with the coordinator,
        let witnesses = state
            .previous_round()
            .ok_or(ApplyError::NoActiveRound)?
            .witnesses;
        let batch_ids = get_batch_ids_for_round(
            state
                .previous_previous_round()
                .ok_or(ApplyError::NoActiveRound)?,
            state,
            previous_round
                .committee_info
                .as_ref()
                .ok_or(ApplyError::NoActiveRound)?
                .2
                .get_num_trainer_nodes(),
        );

        let data_assignments = previous_round.data_assignments.clone();

        Ok(tokio::task::spawn(async move {
                let payloads = payloads.clone();
                let mut distro_results: Vec<Vec<DistroResult>> = Vec::new();

                trace!("Have commitments for batches {:?}", commitments.keys().collect::<Vec<_>>());
                trace!("Have payloads for hashes {:?}", payloads.lock().unwrap().keys().collect::<Vec<_>>());

                for batch_id in batch_ids {
                    let batch_commitments = match commitments.get(&batch_id) {
                        Some(x) => x,
                        None => {
                            let expected_trainer = data_assignments.get(&batch_id);
                            warn!(
                                integration_test_log_marker = %IntegrationTestLogMarker::UntrainedBatches,
                                batch_id = %batch_id,
                                expected_trainer = ?expected_trainer,
                                "No commitments for batch {batch_id}, assigned to node {expected_trainer:?}",
                            );
                            continue;
                        }
                    };
                    trace!("Commitments for batch {batch_id}: {batch_commitments:?}");
                    let consensus = match Coordinator::<T>::select_consensus_commitment_by_witnesses(
                        &batch_commitments
                            .iter()
                            .map(|x| x.1.0)
                            .collect::<Vec<_>>(),
                        &witnesses,
                        witness_quorum,
                    ) {
                        Some(x) => x,
                        None => {
                            warn!("No consensus commitment for batch {}", batch_id);
                            continue;
                        }
                    };
                    trace!("Consensus commitment for batch {batch_id}: {consensus:?}");

                    let (commitment, result) = &batch_commitments[consensus].1;
                    let payload_remove_result = payloads.lock().unwrap().remove(&result.ticket.hash());
                    let maybe_results: Result<(Vec<DistroResult>, u32), DeserializeError> = match payload_remove_result {
                        Some(PayloadState::Deserializing(x)) => match x.is_finished() {
                            true => x.await.unwrap(),
                            false => {
                                return Err(ApplyError::DidNotFinishDeserializingCommitment(
                                    Box::new(*commitment),
                                    batch_id,
                                ));
                            }
                        },
                        Some(PayloadState::Downloading((_, _, ticket))) => {
                            return Err(ApplyError::DidNotBeginDownloadingCommitment(
                                Box::new(*commitment),
                                batch_id,
                                ticket.hash()
                            ));
                        }
                        None => {
                            return Err(ApplyError::UnknownCommitment(
                                Box::new(*commitment),
                                batch_id,
                            ))
                        }
                    };

                    match maybe_results {
                        Ok((results, trainer_nonce)) => {
                            if trainer_nonce < cold_start_warmup_steps && checkpoint_is_p2p {
                                // Only filter results from trainers that are still warming up their optimizer,
                                // and only when the checkpoint is P2P (meaning other clients exist from a previous epoch).
                                // When checkpoint is Hub (first epoch or mass restart), all trainers are equally new so no filtering is needed.
                                // note, we are relying on honest communication of this value here -- will need to harden with verification.
                                info!("Skipping apply of batch {batch_id}, trainer warming up ({trainer_nonce}/{cold_start_warmup_steps})");
                            } else {
                                distro_results.push(results);
                            }
                        }
                        Err(err) => warn!("DESYNC: Got the following error when deserializing results for commitment 0x{}: {}", hex::encode(commitment.data_hash), err),
                    }
                }

                let futures: Vec<JoinHandle<std::result::Result<Trainer, ApplyDistroResultError>>> =
                    trainers
                        .into_iter()
                        .map(|trainer| {
                            let distro_results = Some(distro_results.clone());

                            tokio::task::spawn_blocking(move || {
                                trainer.optimize(step, warmup_lr_between, distro_results)
                            })
                        })
                        .collect::<Vec<_>>();
                let trainers: Vec<_> = try_join_all(futures)
                    .await
                    .map_err(|_| ApplyDistroResultError::ThreadCrashed)?
                    .into_iter()
                    .collect::<Result<_, _>>()?;
                trace!(
                    "Apply time: {:.1}s, {} trainers ready",
                    (Instant::now() - apply_start).as_secs_f32(),
                    trainers.len()
                );
                Ok(trainers)
            }.instrument(trace_span!("Applying distro results"))))
    }
}

fn start_sending_health_checks<T: NodeIdentity>(
    round_state: &mut RoundState<T>,
    state: &Coordinator<T>,
    tx_health_check: mpsc::UnboundedSender<HealthChecks<T>>,
) -> Result<Option<JoinHandle<Result<(), TrainError>>>, TrainError> {
    // we won't have any information to health check with until at least one round of training has finished
    if round_state.height == 0 {
        return Ok(None);
    }
    let (_, witness_proof, committee_selection) = round_state
        .committee_info
        .as_ref()
        .ok_or(TrainError::NoCommitteeInfo)?;
    Ok(
        if state.epoch_state.first_round.is_false() && witness_proof.witness.is_true() {
            let clients = state.epoch_state.clients;
            let committee_selection = committee_selection.clone();
            let state = *state;
            Some(tokio::task::spawn(async move {
                let mut checks = HealthChecks::new();
                for (index, client) in clients.iter().enumerate() {
                    let proof = committee_selection.get_committee(index as u64);
                    if !state.healthy(&client.id, &proof).unwrap_or(false) {
                        warn!(
                            integration_test_log_marker = %IntegrationTestLogMarker::HealthCheck,
                            index = index,
                            client_id = %&client.id,
                            current_step = state.epoch_state.rounds_head,
                            "Found unhealthy trainer at index: {}", index,
                        );
                        checks.push((client.id, proof));
                    }
                }

                if !checks.is_empty() {
                    info!("Sending health check for following indices: {:?}", checks);
                    tx_health_check
                        .send(checks)
                        .map_err(|_| TrainError::SendHealthChecks)
                } else {
                    Ok(())
                }
            }))
        } else {
            None
        },
    )
}

#[derive(Error, Debug)]
pub enum ApplyError {
    #[error("no active round")]
    NoActiveRound,

    #[error("failed to apply distro result: {0}")]
    BadResult(#[from] ApplyDistroResultError),

    #[error("DESYNC: Did not finish deserializing payload for consensus commitment 0x{commitment} for batch {1}", commitment=hex::encode(.0.data_hash))]
    DidNotFinishDeserializingCommitment(Box<Commitment>, BatchId),

    #[error("DESYNC: Did not begin downloading payload for consensus commitment 0x{commitment} for batch {1} with blob hash {2}", commitment=hex::encode(.0.data_hash))]
    DidNotBeginDownloadingCommitment(Box<Commitment>, BatchId, Hash),

    #[error("DESYNC: Unknown consensus commitment 0x{commitment} for batch {1}", commitment=hex::encode(.0.data_hash))]
    UnknownCommitment(Box<Commitment>, BatchId),
}

#[derive(Debug, Error)]
enum WriteGradientsError {
    #[error("Failed to create write_gradients_dir: {0}")]
    CreateDir(tokio::io::Error),

    #[error("Failed to serialize distro result data {fname} to bytes: {err}")]
    Serialize { fname: String, err: postcard::Error },

    #[error("Failed to write distro result data {fname}: {err}")]
    Write {
        fname: String,
        err: tokio::io::Error,
    },
}

async fn write_gradients_to_disk<T: NodeIdentity>(
    write_gradients_dir: PathBuf,
    identity: T,
    distro_result: TransmittableDistroResult,
) -> Result<(), WriteGradientsError> {
    debug!("Trying to write distro result to disk...");
    tokio::fs::create_dir_all(&write_gradients_dir)
        .await
        .map_err(WriteGradientsError::CreateDir)?;

    let fname = format!(
        "result-{}-step{}-batch{}.vec-postcard",
        identity, distro_result.step, distro_result.batch_id
    );
    let fpath = write_gradients_dir.join(&fname);
    let serialized = distro_results_to_bytes(&distro_result.distro_results).map_err(|err| {
        WriteGradientsError::Serialize {
            fname: fname.clone(),
            err,
        }
    })?;
    tokio::fs::write(fpath, serialized)
        .await
        .map_err(|err| WriteGradientsError::Write {
            fname: fname.clone(),
            err,
        })?;
    debug!("Wrote distro result {fname}.");
    Ok(())
}
