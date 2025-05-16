use std::collections::BTreeMap;

use psyche_coordinator::{Client, Coordinator, Witness, WitnessMetadata, SOLANA_MAX_NUM_CLIENTS};
use psyche_core::{FixedVec, MerkleRoot, MerkleTree, NodeIdentity, BatchId};
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

        let sending_witness = if let Some(witness) = WitnessStep::get_witness_to_send(
            previous_round,
            current_round,
            &state.epoch_state.clients,
        ) {
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
        clients: &[Client<T>],
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

        let assigments_vec = Self::calculate_assignments_given_client_times(
            &current_round.client_times,
            &current_round.data_assignments,
            clients,
        );

        let mut assignments: FixedVec<u16, SOLANA_MAX_NUM_CLIENTS> = FixedVec::new();
        assignments.fill(0u16);
        for i in 0..assigments_vec.len() {
            assignments[i] = assigments_vec[i];
        }

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
            client_times: assignments,
        })
    }

    fn calculate_assignments_given_client_times<T: NodeIdentity>(
        client_times: &FixedVec<u16, SOLANA_MAX_NUM_CLIENTS>,
        data_assignments: &BTreeMap<BatchId, T>,
        clients: &[Client<T>],
    ) -> Vec<u16> {
        let mut scores_per_node: Vec<f64> = Vec::new();
        info!("calculate_assigments: client_times: {:?}", client_times);
        info!("calculate_assigments: data_assignments: {:?}", data_assignments);

        for i in 0..clients.len() {
            let batches_assigned =
                Self::number_of_data_assignments_for_client(&clients[i].id, data_assignments);
            info!("batches assigned for client {}: {}", i, batches_assigned);
            let client_time = client_times[i];
            if client_time != 0 {
                let calc = (batches_assigned as f64) / (client_time as f64);
                info!("calculated score for client: {}", calc);
                scores_per_node.push((batches_assigned as f64) / (client_times[i] as f64));
            } else {
                scores_per_node.push(0.0);
            }
        }

        // Step 2: Sum of scores_per_node
        let mut sum: f64 = 0.0;
        for score in &scores_per_node {
            sum += score;
        }

        // Step 3: Normalize scores_per_node
        for i in 0..scores_per_node.len() {
            scores_per_node[i] /= sum;
        }
        dbg!(&scores_per_node);

        // Step 4: Calculate raw_assignments = scores_per_node[i] * total_batch_size
        let mut raw_assignments: Vec<f64> = Vec::new();
        let total_batch_size = 8; // TODO change this obviously
        for score in &scores_per_node {
            raw_assignments.push(score * total_batch_size as f64);
        }
        dbg!(&raw_assignments);

        // Step 5: Floor the assignments
        let mut floored_assignments: Vec<u16> = Vec::new();
        for x in &raw_assignments {
            floored_assignments.push(x.floor() as u16);
        }
        dbg!(&floored_assignments);
        floored_assignments

        // Step 6: Calculate the remainder
        //let mut floored_sum = 0;
        //for val in &floored_assignments {
        //    floored_sum += val;
        //}
        //let remainder = total_batch_size - floored_sum;
    }

    fn number_of_data_assignments_for_client<T: NodeIdentity>(
        client: &T,
        data_assignments: &BTreeMap<BatchId, T>,
    ) -> u16 {
        let mut total: u16 = 0;
        for (assignment, assigned_client) in data_assignments.iter() {
            if assigned_client == client {
                total += assignment.len() as u16;
            }
        }
        total
    }
}
