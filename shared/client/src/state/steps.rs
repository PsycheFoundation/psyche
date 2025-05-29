use crate::{
    client::P2PNodeInfo,
    state::{train::FinishedTrainers, types::DeserializeError},
    Broadcast, BroadcastType, ClientTUIState, IntegrationTestLogMarker,
};

use psyche_coordinator::{Committee, Coordinator, RunState, Witness, WitnessProof};
use psyche_core::{sha256, MerkleRoot, MerkleTree, NodeIdentity};
use psyche_modeling::{DistroResult, Trainer};
use psyche_network::{AuthenticatableIdentity, BlobTicket, Hash, TransmittableDistroResult};
use psyche_watcher::OpportunisticData;
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};
use tch::TchError;
use thiserror::Error;
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};
use tracing::{debug, error, info, trace, trace_span, warn, Instrument};

use super::{
    cooldown::{CooldownError, CooldownStep, CooldownStepMetadata},
    evals::EvalError,
    init::InitRunError,
    round_state::RoundState,
    stats::StatsLogger,
    train::{TrainError, TrainingStep, TrainingStepMetadata},
    types::PayloadState,
    warmup::{WarmupStep, WarmupStepMetadata},
    witness::{WitnessStep, WitnessStepMetadata, WitnessingError},
    FinishedBroadcast, RunInitConfigAndIO,
};

pub struct StepStateMachine<T: NodeIdentity, A: AuthenticatableIdentity + 'static> {
    identity: T,

    stats_logger: Arc<Mutex<StatsLogger>>,

    warmup: WarmupStepMetadata,
    training: TrainingStepMetadata<T, A>,
    witness: WitnessStepMetadata<T>,
    cooldown: CooldownStepMetadata,

    active_step: ActiveStep,

    tx_request_download: mpsc::UnboundedSender<(BlobTicket, u32)>,
    tx_opportunistic_data: mpsc::UnboundedSender<OpportunisticData>,
    tx_broadcast_finished: mpsc::UnboundedSender<FinishedBroadcast>,

    current_round: RoundState<T>,
    previous_round: RoundState<T>,
    step_finish_time: Option<Instant>,
    sent_warmup_finished: bool,
    sent_warmup_witness: bool,

    coordinator_state: Coordinator<T>,
}

#[derive(Error, Debug)]
pub enum StepError {
    #[error("Desync: we're in step {active_step} but next RunState is {run_state}")]
    Desync {
        active_step: String,
        run_state: RunState,
    },

    #[error("Witness error: {0}")]
    Witness(#[from] WitnessingError),

    #[error("Cooldown error: {0}")]
    Cooldown(#[from] CooldownError),

    #[error("Train error: {0}")]
    Train(#[from] TrainError),

    #[error("Evals error: {0}")]
    Evals(#[from] EvalError),

    #[error("Stats logger mutex is poisoned")]
    StatsLoggerMutex,
}

#[derive(Error, Debug)]
pub enum ApplyMessageError {
    #[error("Failed to put blob up for download")]
    StartDownloadBlob,
}

#[derive(Error, Debug)]
pub enum OpportunisticWitnessError {
    #[error("Failed to send opportunistic witness, channel must be closed")]
    Send,

    #[error("Failed to send broadcast finished, channel must be closed")]
    Finished,

    #[error("Stats logger mutex is poisoned")]
    StatsLoggerMutex,

    #[error("Error applying state: {0}")]
    ApplyState(#[from] ApplyStateError),
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> StepStateMachine<T, A> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: T,
        warmup: WarmupStepMetadata,
        training: TrainingStepMetadata<T, A>,
        witness: WitnessStepMetadata<T>,
        cooldown: CooldownStepMetadata,
        trainers: Vec<Trainer>,
        coordinator_state: Coordinator<T>,
        tx_request_download: mpsc::UnboundedSender<(BlobTicket, u32)>,
        tx_opportunistic_data: mpsc::UnboundedSender<OpportunisticData>,
        tx_broadcast_finished: mpsc::UnboundedSender<FinishedBroadcast>,
        stats_logger: StatsLogger,
    ) -> Self {
        let mut previous_round = RoundState::default();
        let mut current_round = RoundState::default();

        let active_step =
            ActiveStep::Warmup(warmup.start(trainers, &mut previous_round, &mut current_round));

        Self {
            identity,

            stats_logger: Arc::new(Mutex::new(stats_logger)),

            warmup,
            training,
            witness,
            cooldown,
            active_step,

            current_round,
            previous_round,

            tx_request_download,
            tx_opportunistic_data,
            tx_broadcast_finished,

            coordinator_state,

            step_finish_time: None,
            sent_warmup_finished: false,
            sent_warmup_witness: false,
        }
    }

    pub fn try_send_opportunistic_witness(&mut self) -> Result<(), OpportunisticWitnessError> {
        if let Some(committee_info) = &self.current_round.committee_info {
            // trace!("Checking for opprotunistic witness with committee info");
            if let ActiveStep::Training(step) = &self.active_step {
                if step.finished() && self.previous_round.batch_ids_not_yet_trained_on.is_none() {
                    // Finished training and finished downloading the previous round's results
                    // (or we're on the first or last which has nothing to download)

                    // check that all batches from the previous round are done deserializing
                    for batch in &self.previous_round.downloads {
                        match batch.1 {
                            PayloadState::Deserializing(thread) if thread.is_finished() => {
                                // this batch is done deserializing, we can witness on it now.
                            }

                            // we're still downloading or deserializing this batch, so we're not ready to send an opportunistic witness.
                            // this function will get called again when a deserialize finishes.
                            _ => {
                                return Ok(());
                            }
                        }
                    }

                    if !self.current_round.sent_finished {
                        // okay, we're all done. we've trained and downloaded everything.
                        // send our early "finished message"

                        let merkle = MerkleTree::new(&self.previous_round.broadcasts)
                            .get_root()
                            .cloned()
                            .unwrap_or(MerkleRoot::default());

                        self.tx_broadcast_finished
                            .send(FinishedBroadcast {
                                step: self.current_round.step,
                                commitment_data_hash: sha256(&merkle.inner),
                                merkle,
                                proof: committee_info.0,
                                warmup: false,
                            })
                            .map_err(|_| OpportunisticWitnessError::Finished)?;

                        self.current_round.sent_finished = true;

                        return Ok(());
                    }

                    // if we get here we've sent our own finished message.
                    // now we just need to wait until we've received everyone else's finished
                    let unfinished_clients: Vec<_> = self
                        .coordinator_state
                        .epoch_state
                        .clients
                        .iter()
                        .filter_map(|client| {
                            if self.current_round.clients_finished.contains_key(&client.id) {
                                None
                            } else {
                                Some(client.id)
                            }
                        })
                        .collect();
                    if !unfinished_clients.is_empty() {
                        return Ok(());
                    }

                    if let Some(witness) = WitnessStep::get_witness_to_send(
                        &mut self.previous_round,
                        &mut self.current_round,
                    ) {
                        info!(target: "witness", id = %self.identity, merkle=witness.broadcast_merkle.fmt_short(), "Sending opportunistic witness");

                        let metadata = self
                            .stats_logger
                            .lock()
                            .map_err(|_| OpportunisticWitnessError::StatsLoggerMutex)?
                            .get_witness_metadata(&self.coordinator_state);
                        self.tx_opportunistic_data
                            .send(OpportunisticData::WitnessStep(witness, metadata))
                            .map_err(|_| OpportunisticWitnessError::Send)?;
                    }
                }
            }
        } else if self.coordinator_state.run_state == RunState::Warmup {
            if !self.sent_warmup_finished {
                let merkle = MerkleTree::new(&self.current_round.broadcasts)
                    .get_root()
                    .cloned()
                    .unwrap_or(MerkleRoot::default());

                info!(name: "send_warmup_broadcast", epoch = self.coordinator_state.progress.epoch, "Sending warmup ready broadcast");
                self.tx_broadcast_finished
                    .send(FinishedBroadcast {
                        step: 0,
                        commitment_data_hash: sha256(&merkle.inner),
                        merkle,
                        proof: Default::default(),
                        warmup: true,
                    })
                    .map_err(|_| OpportunisticWitnessError::Finished)?;

                self.sent_warmup_finished = true;

                return Ok(());
            }

            let unfinished_clients: Vec<_> = self
                .coordinator_state
                .epoch_state
                .clients
                .iter()
                .filter_map(|client| {
                    if self.current_round.clients_finished.contains_key(&client.id) {
                        None
                    } else {
                        Some(client.id)
                    }
                })
                .collect();
            if !unfinished_clients.is_empty() {
                trace!(
                    unfinished_clients = ?unfinished_clients,
                    "Still waiting on {} warmup finish broadcasts",
                    unfinished_clients.len()
                );
                return Ok(());
            }

            if !self.sent_warmup_witness {
                info!(name: "send_warmup_witness", epoch = self.coordinator_state.progress.epoch, "Sending warmup witness");

                let merkle = MerkleTree::new(&self.current_round.broadcasts)
                    .get_root()
                    .cloned()
                    .unwrap_or(MerkleRoot::default());

                if let Some(index) = self
                    .coordinator_state
                    .epoch_state
                    .clients
                    .iter()
                    .position(|x| x.id == self.identity)
                {
                    // coordinator needs to check the index for duplicate detection
                    let index = index as u64;
                    let witness = Witness {
                        proof: WitnessProof {
                            position: index,
                            index,
                            witness: Default::default(),
                        },
                        participant_bloom: Default::default(),
                        broadcast_bloom: Default::default(),
                        broadcast_merkle: merkle,
                    };
                    self.tx_opportunistic_data
                        .send(OpportunisticData::WarmupStep(witness))
                        .map_err(|_| OpportunisticWitnessError::Send)?;
                };

                self.sent_warmup_witness = true;
            }
        }
        Ok(())
    }

    pub async fn apply_message(
        &mut self,
        from_client_id: T,
        broadcast: Broadcast,
    ) -> Result<(), ApplyMessageError> {
        let result_step = broadcast.step;

        let round_state = if self.current_round.step == broadcast.step {
            &mut self.current_round
        } else if self.previous_round.step == broadcast.step {
            &mut self.previous_round
        } else {
            trace!(
                "Unknown round for gossiped, says it's for step {} but our current round is step {} and previous round is step {}",
                result_step, self.current_round.step, self.previous_round.step,
            );
            return Ok(());
        };

        let is_warmup_broadcast = match &broadcast.data {
            BroadcastType::TrainingResult(_) => false,
            BroadcastType::Finished(finished) => finished.warmup,
        };

        let check_committee = !is_warmup_broadcast && from_client_id != self.identity;
        if check_committee {
            match &round_state.committee_info {
                Some((_, _, committee_info)) => {
                    if !committee_info.verify_committee_for_client(
                        &from_client_id,
                        &broadcast.proof,
                        &self.coordinator_state.epoch_state.clients,
                    ) {
                        debug!("Committee verification failed for commitment 0x{} (step={}) received from {}", hex::encode(broadcast.commitment.data_hash),
                            broadcast.step,
                            from_client_id);
                        return Ok(());
                    }
                }
                None => {
                    return Ok(());
                }
            };
        } else if !self
            .coordinator_state
            .epoch_state
            .clients
            .iter()
            .any(|x| x.id == from_client_id)
        {
            debug!(
                "Client verification failed for commitment 0x{} (step={}) received from {}",
                hex::encode(broadcast.commitment.data_hash),
                broadcast.step,
                from_client_id
            );
            return Ok(());
        }

        if !is_warmup_broadcast && broadcast.proof.committee != Committee::Trainer {
            debug!(
                "Broadcast not implemented for committee member {}",
                broadcast.proof.committee
            );
            return Ok(());
        }

        match broadcast.data {
            BroadcastType::TrainingResult(training_result) => {
                if !round_state
                    .data_assignments
                    .contains_key(&training_result.batch_id)
                {
                    debug!(
                        "Training result for step {} batch id {} is not in our data assignments",
                        broadcast.step, training_result.batch_id
                    );
                    return Ok(());
                }
                let ticket = training_result.ticket.clone();
                let hash = ticket.hash();

                if round_state.downloads.contains_key(&hash) {
                    trace!(
                        "Already have downloaded batch id {}, ignoring duplicated gossip",
                        training_result.batch_id
                    );
                    return Ok(());
                }

                let correct_assignee =
                    match round_state.data_assignments.get(&training_result.batch_id) {
                        Some(assignee) => from_client_id == *assignee,
                        None => false,
                    };
                if !correct_assignee {
                    warn!(
                            "Got batch {} from {} but they were not assigneed to that data, dropping message 0x{}",
                            training_result.batch_id,
                            from_client_id,
                            hex::encode(broadcast.commitment.data_hash)
                        );
                    return Ok(());
                }

                round_state
                    .results
                    .entry(training_result.batch_id)
                    .or_default();
                let batch_id = training_result.batch_id;
                round_state
                    .results
                    .get_mut(&training_result.batch_id)
                    .unwrap()
                    .push((from_client_id, (broadcast.commitment, training_result)));
                let download_state =
                    PayloadState::Downloading((from_client_id, batch_id, ticket.clone()));
                round_state.downloads.insert(hash, download_state);

                // start downloading the payload unless this is a self-message
                // (assuming the caller will put our payload in the proper place)
                if from_client_id != self.identity {
                    self.tx_request_download
                        .send((ticket, result_step))
                        .map_err(|_| ApplyMessageError::StartDownloadBlob)?;
                }
            }
            BroadcastType::Finished(finished) => {
                if round_state.clients_finished.contains_key(&from_client_id) {
                    trace!(
                        "Already got finished broadcast from {}, ignorning",
                        from_client_id
                    );
                    return Ok(());
                }

                round_state
                    .clients_finished
                    .insert(from_client_id, finished.clone());

                if finished.warmup {
                    debug!(
                        "Received {} warmup readies",
                        round_state.clients_finished.len(),
                    );
                } else {
                    trace!(
                        "Received {} finishes for round {}",
                        round_state.clients_finished.len(),
                        result_step
                    );
                }
            }
        }

        round_state.broadcasts.push(broadcast.commitment.data_hash);

        Ok(())
    }

    pub async fn apply_distro_result(
        &mut self,
        hash: Hash,
        distro_result: TransmittableDistroResult,
        self_result: Option<Vec<DistroResult>>,
    ) {
        let round_state = if self.current_round.downloads.contains_key(&hash) {
            trace!(
                "Got download {hash} for current round {}",
                self.current_round.height
            );
            &mut self.current_round
        } else if self.previous_round.downloads.contains_key(&hash) {
            trace!(
                "Got download {hash} for previous round {}",
                self.previous_round.height
            );
            &mut self.previous_round
        } else {
            warn!("Unknown download {}", hash);
            return;
        };

        if let Some(self_result) = self_result {
            trace!(
                "Processing our own distro result for batch {} in step {} with hash {hash}",
                distro_result.batch_id,
                distro_result.step
            );

            round_state.self_distro_results.push(self_result);
        } else {
            trace!(
                "Finished download of distro result for batch {} in step {} with hash {hash}",
                distro_result.batch_id,
                distro_result.step
            );
        }

        let (from, batch_id, _) = match round_state.downloads.get(&hash) {
            Some(PayloadState::Downloading(x)) => x.clone(),
            Some(PayloadState::Deserializing(_)) => {
                debug!("Duplicate download of {}", hash);
                return;
            }
            None => {
                debug!("Unknown download {}", hash);
                return;
            }
        };

        let commitments = match round_state.results.get(&batch_id) {
            Some(commitments) => commitments,
            None => {
                info!(
                    "No commitment for payload from {} for batch {}",
                    from, batch_id
                );
                return;
            }
        };
        let commitment = match commitments
            .iter()
            .find(|x| x.0 == from && x.1 .1.ticket.hash() == hash)
        {
            Some(commitment) => &commitment.1 .0,
            None => {
                info!("No commitment for payload from {}", from);
                return;
            }
        };

        // verify that the result matches the commitment

        let (distro_hash, distro_result) =
            tokio::task::spawn_blocking(move || (distro_result.comptue_hash(), distro_result))
                .await
                .unwrap();

        if distro_hash != commitment.data_hash {
            debug!(
                from = %from,
                batch_id = %batch_id,
                "Distro result failed commitment hash verification",
            );
            return;
        }

        // TODO: verify shape of distro_results

        // we only care to add this to consensus & track it in batch IDs if we have any batch IDs that haven't yet been voted for.
        // TODO: how do we do witnessing for verifiers that might be training on data that's not in the normal remaining batch IDs?
        // TODO: also we want ALL those from everyone, right?
        let just_finished = if let Some((num_batch_ids_left, batch_ids_not_yet_trained_on)) =
            &mut round_state.batch_ids_not_yet_trained_on
        {
            let mut remaining_batch_ids = batch_ids_not_yet_trained_on.lock().await;

            match round_state.blooms.as_mut() {
                Some((participant_bloom, broadcast_bloom)) => {
                    participant_bloom.add(&sha256(from.as_ref()));
                    if remaining_batch_ids.contains(&batch_id) {
                        // first received payload for this batch id, vote for it in consensus
                        broadcast_bloom.add(&commitment.data_hash);
                        trace!("Adding batch {batch_id} to broadcast bloom");
                    } else {
                        trace!(
                            "Don't have {} in our remaining batch IDs {:?}, discarding",
                            batch_id,
                            remaining_batch_ids
                        );
                    }
                }
                None => {
                    trace!(
                        "Already submitted witness, not adding {} to participant bloom",
                        from
                    );
                }
            }

            remaining_batch_ids.remove(&batch_id);

            trace!(
                "Remaining batches to download for step {}: {:?}",
                distro_result.step,
                remaining_batch_ids
            );

            *num_batch_ids_left = remaining_batch_ids.len();

            remaining_batch_ids.is_empty()
        } else {
            trace!("All batches already trained on, discarding batch {batch_id}");
            false
        };

        if just_finished {
            round_state.batch_ids_not_yet_trained_on = None;
        }

        // we unconditionally store every seen payload, since we're not yet sure what consensus will be on whether it's included.
        let deserializing = tokio::task::spawn({
            //let batch_id = *batch_id;
            async move {
                let maybe_results = tokio::task::spawn_blocking(move || {
                    let r = distro_result
                        .distro_results
                        .iter()
                        .map(|x| x.try_into())
                        .collect::<Result<Vec<DistroResult>, TchError>>()
                        .map(|x| (x, distro_result.trainer_nonce));
                    trace!(
                        hash = %hash,
                        batch_id = %batch_id,
                        "Finished deserializing payload {} for batch {}",
                        hash,
                        batch_id
                    );
                    r
                })
                .await
                .map_err(|_| DeserializeError::DeserializeThreadCrashed)??;
                Ok(maybe_results)
            }
        });

        round_state
            .downloads
            .insert(hash, PayloadState::Deserializing(deserializing));
    }

    async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), StepError> {
        let client_index = match state
            .epoch_state
            .clients
            .iter()
            .position(|x| x.id == self.identity)
        {
            Some(index) => index as u64,
            None => {
                trace!(
                    "saw new step, but we're not one of the clients. our id: {}, all clients: {:?}",
                    self.identity,
                    &state
                        .epoch_state
                        .clients
                        .iter()
                        .map(|c| c.id)
                        .collect::<Vec<_>>()
                );
                let new_step = match std::mem::take(&mut self.active_step) {
                    ActiveStep::Intermediate => {
                        unreachable!("can never be in intermediate state.")
                    }
                    ActiveStep::Warmup(warmup) => ActiveStep::Warmup(warmup),
                    ActiveStep::Cooldown(cooldown) => {
                        trace!("since we're not a member of this step, killing cooldown step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            cooldown.finish().await?,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                    ActiveStep::Training(training) => {
                        trace!("since we're not a member of this step, killing training step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            training.finish().await?.evals_or_trainers,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                    ActiveStep::Witness(witness) => {
                        trace!("since we're not a member of this step, killing witness step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            witness.finish().await?,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                };
                self.active_step = new_step;

                return Ok(());
            }
        };

        let new_step: ActiveStep = match (std::mem::take(&mut self.active_step), state.run_state) {
            // start training at the beginning of an epoch
            (ActiveStep::Warmup(warmup), RunState::RoundTrain) => {
                let trainers = warmup.finish().stop_evals().await?;
                self.step_finish_time = None;
                self.sent_warmup_finished = false;
                self.sent_warmup_witness = false;
                self.stats_logger
                    .lock()
                    .map_err(|_| StepError::StatsLoggerMutex)?
                    .push_eval_results();
                ActiveStep::Training(self.training.start(
                    client_index,
                    &state,
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                )?)
            }

            // start witnessing after training is done
            (ActiveStep::Training(training), RunState::RoundWitness) => {
                let FinishedTrainers {
                    evals_or_trainers,
                    round_losses,
                    optim_stats,
                    round_duration,
                } = training.finish().await?;
                let step_duration = self
                    .step_finish_time
                    .map(|step_finish_time| Instant::now() - step_finish_time);
                self.step_finish_time = Some(Instant::now());
                let loss = self
                    .stats_logger
                    .lock()
                    .map_err(|_| StepError::StatsLoggerMutex)?
                    .push_round_stats(&round_losses, round_duration, step_duration, optim_stats);
                info!(
                    integration_test_log_marker = %IntegrationTestLogMarker::Loss,
                    client_id = %self.identity,
                    epoch = state.progress.epoch,
                    step = state.progress.step,
                    loss = loss.unwrap_or(f32::NAN),
                    "client_loss",
                );
                self.stats_logger
                    .lock()
                    .map_err(|_| StepError::StatsLoggerMutex)?
                    .publish_round_stats(&state);
                let witness_metadata = self
                    .stats_logger
                    .lock()
                    .map_err(|_| StepError::StatsLoggerMutex)?
                    .get_witness_metadata(&state);
                ActiveStep::Witness(self.witness.start(
                    client_index,
                    &state,
                    evals_or_trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                    witness_metadata,
                )?)
            }
            // within an epoch, loop back to training after witnessing
            (ActiveStep::Witness(witnessing), RunState::RoundTrain) => {
                let trainers = witnessing.finish().await?.stop_evals().await?;
                ActiveStep::Training(self.training.start(
                    client_index,
                    &state,
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                )?)
            }

            // the epoch ended & we're transitioning to cooldown
            (ActiveStep::Witness(witnessing), RunState::Cooldown) => {
                let trainers = witnessing.finish().await?.stop_evals().await?;
                ActiveStep::Cooldown(self.cooldown.start(trainers, &state)?)
            }
            // cooldown is done, we consider waiting for members and warmup to be basically the same
            (ActiveStep::Cooldown(cooldown), RunState::WaitingForMembers)
            | (ActiveStep::Cooldown(cooldown), RunState::Warmup)
            | (ActiveStep::Cooldown(cooldown), RunState::Paused) => {
                let trainers = cooldown.finish().await?;
                ActiveStep::Warmup(self.warmup.start(
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                ))
            }
            // stay in existing run state if there's no reason to change.
            (current_step, next_run_state) if current_step.allowed_in_run_state(next_run_state) => {
                current_step
            }
            // but if it's not allowed in this run state, we've desynced.
            (current_step, next_run_state) => {
                let step_error = StepError::Desync {
                    active_step: current_step.to_string(),
                    run_state: next_run_state,
                };
                debug!("DESYNC: {step_error}");
                return Err(step_error);
            }
        };
        self.active_step = new_step;
        self.coordinator_state = state;

        Ok(())
    }

    pub fn set_node_info(&mut self, node_info: HashMap<String, P2PNodeInfo>) -> anyhow::Result<()> {
        self.stats_logger
            .lock()
            .map_err(|_| anyhow::anyhow!("stats logger mutex poisoned"))?
            .node_info = node_info;
        Ok(())
    }
}

#[derive(Default, Debug)]
enum ActiveStep {
    #[default]
    Intermediate,

    Warmup(WarmupStep),
    Training(TrainingStep),
    Witness(WitnessStep),
    Cooldown(CooldownStep),
}

impl ActiveStep {
    pub fn allowed_in_run_state(&self, run_state: RunState) -> bool {
        match (self, run_state) {
            (ActiveStep::Intermediate, _) => {
                unreachable!("the intermediate run state can never be seen, it's ephemeral")
            }
            (
                ActiveStep::Warmup(..),
                RunState::Warmup | RunState::WaitingForMembers | RunState::Paused,
            ) => true,
            (ActiveStep::Cooldown(..), RunState::Cooldown) => true,
            (ActiveStep::Training(..), RunState::RoundTrain) => true,
            (ActiveStep::Witness(..), RunState::RoundWitness) => true,
            _ => false,
        }
    }
}

impl fmt::Display for ActiveStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveStep::Intermediate => write!(f, "Intermediate"),
            ActiveStep::Warmup(_) => write!(f, "Warmup"),
            ActiveStep::Training(_) => write!(f, "Training"),
            ActiveStep::Witness(_) => write!(f, "Witness"),
            ActiveStep::Cooldown(_) => write!(f, "Cooldown"),
        }
    }
}

pub enum InitStage<T: NodeIdentity, A: AuthenticatableIdentity + 'static> {
    NotYetInitialized(Option<RunInitConfigAndIO<T, A>>),
    #[allow(clippy::type_complexity)]
    Initializing(
        (
            JoinHandle<Result<StepStateMachine<T, A>, InitRunError>>,
            Coordinator<T>,
        ),
    ),
    Running(StepStateMachine<T, A>),
}

pub struct RunManager<T: NodeIdentity, A: AuthenticatableIdentity + 'static>(InitStage<T, A>);

#[derive(Error, Debug)]
pub enum ApplyStateError {
    #[error("Failed to init run in warmup: {0}")]
    Init(InitRunError),

    #[error("Failed to run step: {0}")]
    Step(#[from] StepError),
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> RunManager<T, A> {
    pub fn new(config: RunInitConfigAndIO<T, A>) -> Self {
        Self(InitStage::NotYetInitialized(Some(config)))
    }

    pub fn coordinator_state(&self) -> Option<&Coordinator<T>> {
        match &self.0 {
            InitStage::NotYetInitialized(..) => None,
            InitStage::Initializing(init_state) => Some(&init_state.1),
            InitStage::Running(step_state_machine) => Some(&step_state_machine.coordinator_state),
        }
    }

    pub async fn try_send_opportunistic_witness(
        &mut self,
    ) -> Result<(), OpportunisticWitnessError> {
        if let InitStage::Initializing((_init_future, init_state)) = &mut self.0 {
            // if we're still initializing, check to see if we're done
            let init_state = *init_state;
            self.apply_state(init_state).await?;
        }
        if let InitStage::Running(state_machine) = &mut self.0 {
            state_machine.try_send_opportunistic_witness()?;
        }
        Ok(())
    }

    pub async fn apply_message(
        &mut self,
        from_client_id: T,
        training_result: Broadcast,
    ) -> Result<(), ApplyMessageError> {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .apply_message(from_client_id, training_result)
                    .await
            }
            _ => {
                // not yet warmed up, ignore any p2p messages.
                Ok(())
            }
        }
    }

    pub async fn apply_distro_result(
        &mut self,
        hash: psyche_network::Hash,
        distro_result: TransmittableDistroResult,
        self_result: Option<Vec<DistroResult>>,
    ) {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .apply_distro_result(hash, distro_result, self_result)
                    .await;
            }
            _ => {
                // not yet warmed up, ignore any p2p messages.
            }
        }
    }

    pub async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), ApplyStateError> {
        let new_state = match &mut self.0 {
            InitStage::NotYetInitialized(init_info @ Some(..))
            // We run the initialization only when we are sure that we didn't just recently joined in Warmup
            // If this is the case, then our ID won't be present in the list of clients available for this epoch
                if state.run_state == RunState::Warmup && state.epoch_state.clients.iter().any(|c| c.id == init_info.as_ref().unwrap().init_config.identity) =>
            {
                // Take ownership of init_info using std::mem::take
                let init_info = init_info.take().unwrap();
                Some(InitStage::Initializing((
                    tokio::spawn(init_info.init_run(state)),
                    state,
                )))
            }
            InitStage::NotYetInitialized(None) => {
                unreachable!("Once we take the init state, we move to initializing.");
            }
            InitStage::Initializing(..)
                if state.run_state == RunState::WaitingForMembers
                    || state.run_state == RunState::Paused =>
            {
                // a client has left the network, transitioning back to RunState::WaitingForMembers.
                // wait for new clients to join the network.
                return Ok(());
            }
            InitStage::Initializing((ref mut init_future, _)) => {
                // Try to complete initialization
                match init_future.is_finished() {
                    true => match init_future.await.unwrap() {
                        Ok(state_machine) => Some(InitStage::Running(state_machine)),
                        Err(e) => {
                            return Err(ApplyStateError::Init(e));
                        }
                    },
                    false => {
                        // We're still initializing, keep current state
                        return Ok(());
                    }
                }
            }
            // we're running, process it in a sec
            InitStage::Running(..) => None,
            // not initialized but we haven't seen a warmup yet, we're just waiting!
            InitStage::NotYetInitialized(_) => {
                return Ok(());
            }
        };

        if let Some(new_state) = new_state {
            self.0 = new_state;
        }

        // yay ok new state! let's go!
        if let InitStage::Running(state_machine) = &mut self.0 {
            state_machine
                .apply_state(state)
                .instrument(trace_span!("StepStateMachine::apply_state"))
                .await?;
        }

        Ok(())
    }

    pub fn stats(&self) -> Option<Arc<Mutex<StatsLogger>>> {
        match &self.0 {
            InitStage::Running(run) => Some(run.stats_logger.clone()),
            _ => None,
        }
    }

    pub fn set_node_info(&mut self, node_info: HashMap<String, P2PNodeInfo>) -> anyhow::Result<()> {
        if let InitStage::Running(run) = &mut self.0 {
            run.set_node_info(node_info)?;
        }
        Ok(())
    }

    pub fn doing_checkpoint(&self) -> bool {
        match &self.0 {
            InitStage::Running(step_state_machine) => match &step_state_machine.active_step {
                ActiveStep::Cooldown(cooldown_step) => cooldown_step.doing_checkpoint(),
                _ => false,
            },
            _ => false,
        }
    }
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> From<&RunManager<T, A>>
    for ClientTUIState
{
    fn from(run: &RunManager<T, A>) -> Self {
        match &run.0 {
            InitStage::Running(state_machine) => {
                let coordinator = &state_machine.coordinator_state;
                let committee = state_machine
                    .current_round
                    .committee_info
                    .as_ref()
                    .map(|x| x.0.committee);
                let stats = run.stats();
                let stats = stats.as_ref();
                let stats_guard = stats.and_then(|s| s.lock().ok());

                ClientTUIState {
                    step: coordinator.progress.step,
                    committee,
                    run_state: coordinator.into(),
                    loss: stats_guard
                        .as_ref()
                        .map(|s| s.losses().to_vec())
                        .unwrap_or_default(),
                    batches_left: state_machine
                        .current_round
                        .batch_ids_not_yet_trained_on
                        .as_ref()
                        .map(|x| x.0)
                        .unwrap_or_default(),
                    global_tokens_per_second: stats_guard
                        .as_ref()
                        .map(|s| s.global_tokens_per_second(coordinator))
                        .unwrap_or_default(),
                    efficency: stats_guard
                        .as_ref()
                        .map(|x| x.efficency())
                        .unwrap_or_default(),
                    total_tokens: coordinator.total_tokens_processed(coordinator.current_round()),
                    evals: stats_guard
                        .as_ref()
                        .map(|s| s.eval_history().clone())
                        .unwrap_or_default(),
                    token_batch_size: coordinator.get_sequence_length()
                        * coordinator.get_target_global_batch_size(coordinator.current_round())
                            as u32,
                }
            }
            _ => Default::default(),
        }
    }
}
