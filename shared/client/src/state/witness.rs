use super::{
    evals::{EvalError, MaybeRunningEvals, ModelTaskRunner, RunningEvals},
    round_state::RoundState,
};
use psyche_coordinator::{
    coordinator::{Coordinator, Witness},
    hash_wrapper::HashWrapper,
    node_identity::NodeIdentity,
};
use psyche_core::{MerkleTree, WitnessMetadata};
use psyche_watcher::OpportunisticData;
use thiserror::Error;
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};
use tracing::{info, trace};

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

pub struct WitnessStepMetadata {
    pub identity: NodeIdentity,
    pub model_task_runner: ModelTaskRunner,
    pub tx_witness: mpsc::UnboundedSender<OpportunisticData>,
}

#[derive(Debug)]
pub struct WitnessStep {
    evals: RunningEvals,
    sending_witness: Option<JoinHandle<Result<(), WitnessingError>>>,
}

impl WitnessStepMetadata {
    pub fn start(
        &self,
        _client_index: u64,
        _state: &Coordinator,
        trainers: MaybeRunningEvals,
        previous_round: &mut RoundState,
        current_round: &mut RoundState,
        metadata: WitnessMetadata,
    ) -> Result<WitnessStep, WitnessingError> {
        if trainers.is_empty() {
            return Err(WitnessingError::NoTrainers);
        }

        let evals = self.model_task_runner.start_if_not_running(trainers);

        let sending_witness = if let Some(witness) =
            WitnessStep::get_witness_to_send(previous_round, current_round)
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

    pub fn get_witness_to_send(
        previous_round: &mut RoundState,
        current_round: &mut RoundState,
    ) -> Option<Witness> {
        if previous_round.sent_witness {
            return None;
        }

        let (_, proof, _) = current_round.committee_info.as_ref()?;
        if proof.witness.is_false() {
            return None;
        }

        let merkle = MerkleTree::new(&previous_round.broadcasts);
        let broadcast_merkle = merkle.get_root().cloned().unwrap_or(HashWrapper::default());

        let (participant_bloom, broadcast_bloom) =
            previous_round.blooms.lock().unwrap().unwrap_or_default();

        info!("Submitting witness blooms");
        previous_round.sent_witness = true;

        trace!("Participant bloom: {:?}", participant_bloom);
        trace!("Broadcast bloom: {:?}", broadcast_bloom);
        trace!("Merkle root: 0x{}", hex::encode(broadcast_merkle.inner));

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
        })
    }
}
