use psyche_coordinator::{Coordinator, Witness, WitnessMetadata, TRAINING_TIMES_SLICE_SIZE};
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

        // Calculate the window bounds
        let clients_len = state.epoch_state.clients.len();
        let client_index_start = state.epoch_state.time_witnessing_window_start as usize;
        let client_index_end = client_index_start + TRAINING_TIMES_SLICE_SIZE - 1;

        trace!(
            "[get_witness_to_send] Submitting training times for step={}, witnessing times for window [{}:{}]",
            state.progress.step,
            client_index_start,
            client_index_end,
        );

        let mut training_times: FixedVec<u16, TRAINING_TIMES_SLICE_SIZE> = FixedVec::new();
        for i in 0..=client_index_end {
            let source_idx = client_index_start + i;

            if source_idx < clients_len {
                let _ = training_times.push(
                    current_round
                        .client_times
                        .get(source_idx)
                        .copied()
                        .unwrap_or(0),
                );
            }
            // If source_idx is out of bounds, training_times[i] remains 0
        }

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
            training_times,
        })
    }
}
