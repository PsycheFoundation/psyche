use std::sync::{Arc, Mutex};

use psyche_coordinator::{Coordinator, Witness, WitnessMetadata};
use psyche_core::{MerkleRoot, MerkleTree, NodeIdentity};
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
        _state: &Coordinator<T>,
        trainers: MaybeRunningEvals,
        previous_round: Arc<Mutex<RoundState<T>>>,
        current_round: Arc<Mutex<RoundState<T>>>,
        metadata: WitnessMetadata,
    ) -> Result<WitnessStep, WitnessingError> {
        if trainers.is_empty() {
            return Err(WitnessingError::NoTrainers);
        }

        let evals = self.eval_runner.start_if_not_running(trainers);

        let mut previous_round = previous_round.lock().unwrap();
        let mut current_round = current_round.lock().unwrap();
        let sending_witness = if let Some(witness) =
            WitnessStep::get_witness_to_send(&mut previous_round, &mut current_round)
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

        let blooms = previous_round.blooms;
        let (participant_bloom, broadcast_bloom) = blooms.unwrap_or_default();

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
