use psyche_coordinator::{
    Coordinator, Witness, WitnessMetadata, WitnessTrainingTimes, SOLANA_MAX_NUM_CLIENTS,
    TRAINING_TIMES_SLICE_SIZE,
};
use psyche_core::{FixedVec, MerkleRoot, MerkleTree, NodeIdentity};
use psyche_watcher::OpportunisticData;
use thiserror::Error;
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};
use tracing::{info, trace};

use super::{
    evals::{EvalError, EvalRunner, MaybeRunningEvals, RunningEvals},
    round_state::RoundState,
};

#[derive(Debug, Error)]
pub enum WitnessingError {
    #[error("Failed to stop evals")]
    StopEvals(#[from] EvalError),

    #[error("Couldn't start evals - no trainers passed to us")]
    NoTrainers,

    #[error("Failed to send witness, channel closed?")]
    Send,

    #[error("Witness send thread crashed")]
    SendThreadCrashed,
}

pub struct WitnessStepMetadata<T: NodeIdentity> {
    pub identity: T,
    pub eval_runner: EvalRunner,
    pub tx_witness: mpsc::UnboundedSender<OpportunisticData>,
}

#[derive(Debug)]
pub struct WitnessStep {
    evals: RunningEvals,
    sending_witness: Option<JoinHandle<Result<(), WitnessingError>>>,
}

impl<T: NodeIdentity> WitnessStepMetadata<T> {
    pub fn start(
        &self,
        _client_index: u64,
        state: &Coordinator<T>,
        trainers: MaybeRunningEvals,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
        metadata: WitnessMetadata,
    ) -> Result<WitnessStep, WitnessingError> {
        if trainers.is_empty() {
            return Err(WitnessingError::NoTrainers);
        }

        let evals = self.eval_runner.start_if_not_running(trainers);

        let sending_witness = if let Some(witness) =
            WitnessStep::get_witness_to_send(state, previous_round, current_round)
        {
            let tx_witness = self.tx_witness.clone();
            Some(tokio::task::spawn(async move {
                tx_witness
                    .send(OpportunisticData::WitnessStep(witness, metadata))
                    .map_err(|_| WitnessingError::Send)
            }))
        } else {
            None
        };
        Ok(WitnessStep {
            evals,
            sending_witness,
        })
    }
}

impl WitnessStep {
    pub async fn finish(self) -> Result<RunningEvals, WitnessingError> {
        if let Some(witness_thread) = self.sending_witness {
            witness_thread
                .await
                .map_err(|_| WitnessingError::SendThreadCrashed)??;
        }
        Ok(self.evals)
    }

    pub fn get_witness_to_send<T: NodeIdentity>(
        state: &Coordinator<T>,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> Option<Witness> {
        trace!("Coordinator step: {}", state.progress.step);
        if previous_round.sent_witness {
            return None;
        }

        let (_, proof, _) = current_round.committee_info.as_ref()?;
        if proof.witness.is_false() {
            return None;
        }

        let merkle = MerkleTree::new(&previous_round.broadcasts);
        let broadcast_merkle = merkle.get_root().cloned().unwrap_or(MerkleRoot::default());

        let (participant_bloom, broadcast_bloom) =
            previous_round.blooms.lock().unwrap().unwrap_or_default();

        info!("Submitting witness blooms");
        previous_round.sent_witness = true;

        trace!("Participant bloom: {:?}", participant_bloom);
        trace!("Broadcast bloom: {:?}", broadcast_bloom);
        trace!("Merkle root: 0x{}", hex::encode(broadcast_merkle.inner));

        // let n_partitions = SOLANA_MAX_NUM_CLIENTS.div_ceil(TRAINING_TIMES_SLICE_SIZE);
        let n_partitions = if state
            .epoch_state
            .clients
            .len()
            .rem_euclid(TRAINING_TIMES_SLICE_SIZE)
            == 0
        {
            state.epoch_state.clients.len() / TRAINING_TIMES_SLICE_SIZE
        } else {
            state.epoch_state.clients.len() / TRAINING_TIMES_SLICE_SIZE + 1
        };
        let step = state.progress.step - 1; // First step is 1, so we subtract 1 to get 0-index

        // Cycle through the partitions depending on the current step.
        // let slice_index = if n_partitions == 1 {
        //     0
        // } else {
        //     step % (n_partitions as u32)
        // };
        let slice_index = step % (n_partitions as u32);

        // Calculate the starting index in current_round.client_times for the current slice.
        let start_idx_in_client_times = (slice_index as usize) * TRAINING_TIMES_SLICE_SIZE;
        trace!(
            "Submitting training times for step={} , offset={}",
            step,
            start_idx_in_client_times
        );

        let mut training_times: FixedVec<u16, TRAINING_TIMES_SLICE_SIZE> = FixedVec::new_filled(0);
        for i in 0..(TRAINING_TIMES_SLICE_SIZE) {
            let source_idx = start_idx_in_client_times + i;

            // Check if the source index is within the bounds of current_round.client_times.
            if source_idx < current_round.client_times.len() {
                training_times[i] = current_round
                    .client_times
                    .get(source_idx)
                    .copied()
                    .unwrap_or(0);
            }
            // If source_idx is out of bounds, training_times[i] remains 0
        }

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
            training_times: WitnessTrainingTimes {
                offset: start_idx_in_client_times as u8,
                times: training_times,
            },
        })
    }
}
