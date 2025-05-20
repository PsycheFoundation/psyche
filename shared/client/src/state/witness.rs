use std::collections::BTreeMap;

use psyche_coordinator::{Client, Coordinator, Witness, WitnessMetadata, SOLANA_MAX_NUM_CLIENTS};
use psyche_core::{BatchId, FixedVec, MerkleRoot, MerkleTree, NodeIdentity};
use psyche_watcher::OpportunisticData;
use thiserror::Error;
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};
use tracing::{error, info, trace};

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

        let mut proposed_batch_sizes: FixedVec<u16, SOLANA_MAX_NUM_CLIENTS> = FixedVec::new();
        let _ = proposed_batch_sizes.fill(0u16);
        for i in 0..assigments_vec.len() {
            proposed_batch_sizes[i] = assigments_vec[i];
        }

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
            proposed_batch_sizes,
        })
    }

    fn calculate_assignments_given_client_times<T: NodeIdentity>(
        client_times: &FixedVec<u16, SOLANA_MAX_NUM_CLIENTS>,
        data_assignments: &BTreeMap<BatchId, T>,
        clients: &[Client<T>],
    ) -> Vec<u16> {
        let global_batch_size = 8; // TODO change this obviously
        let mut scores_per_node: Vec<f64> = Vec::new();
        info!("[calculate_assignments] client_times: {:?}", client_times);
        info!(
            "[calculate_assignments] data_assignments: {:?}",
            data_assignments
        );

        for i in 0..clients.len() {
            let batches_assigned =
                Self::number_of_data_assignments_for_client(&clients[i].id, data_assignments);

            let score = if client_times[i] != 0 {
                (batches_assigned as f64) / (client_times[i] as f64)
            } else {
                0.0
            };
            scores_per_node.push(score);

            info!(
                "[calculate_assignments] client {} ({})\n
                client_time: {:?} , batches trained: {}, calculated score: {}\n",
                i, clients[i].id, client_times[i], batches_assigned, score,
            );
        }

        // Step 2: Sum of scores_per_node
        let sum = scores_per_node.iter().sum::<f64>();
        dbg!(sum);

        if sum.abs() < 1e-10f64 {
            error!("client_times is empty, using equitative assignments");
            let assignments =
                Self::calculate_equitative_assignments(global_batch_size, clients.len() as u16);
            info!("equitative assignments: {:?}", assignments);
            return assignments;
        }

        // Initialize final assignments
        let mut final_assignments = vec![0u16; clients.len()];
        
        // First pass: assign 1 batch to each node with non-zero score
        let mut remaining_batches = global_batch_size;
        for (i, &score) in scores_per_node.iter().enumerate() {
            if score > 0.0 {
                final_assignments[i] = 1;
                remaining_batches -= 1;
            }
        }

        if remaining_batches > 0 {
            // Normalize scores for remaining distribution
            let sum = scores_per_node.iter().sum::<f64>();
            scores_per_node.iter_mut().for_each(|score| *score /= sum);

            // Calculate raw assignments for remaining batches
            let raw_remaining: Vec<f64> = scores_per_node
                .iter()
                .map(|&score| score * remaining_batches as f64)
                .collect();

            // Floor the remaining assignments
            let mut additional = raw_remaining
                .iter()
                .map(|&x| x.floor() as u16)
                .collect::<Vec<u16>>();

            // Calculate how many batches are still unassigned
            let assigned: u16 = additional.iter().sum();
            let still_remaining = remaining_batches - assigned;

            // Distribute remaining batches by fractional part
            if still_remaining > 0 {
                let mut fractional: Vec<(usize, f64)> = raw_remaining
                    .iter()
                    .enumerate()
                    .map(|(i, &x)| (i, x - x.floor()))
                    .collect();
                fractional.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                for (idx, _) in fractional.iter().take(still_remaining as usize) {
                    additional[*idx] += 1;
                }
            }

            // Add additional assignments to base assignments
            for (base, extra) in final_assignments.iter_mut().zip(additional.iter()) {
                *base += *extra;
            }
        }

        dbg!(&final_assignments);
        final_assignments
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

    fn calculate_equitative_assignments(
        global_batch_size: u16,
        number_of_clients: u16,
    ) -> Vec<u16> {
        let mut assignments: Vec<u16> = vec![0; number_of_clients as usize];
        let mut total_assigned = 0;

        for i in 0..number_of_clients {
            assignments[i as usize] = global_batch_size / number_of_clients;
            total_assigned += assignments[i as usize];
        }

        let remainder = global_batch_size - total_assigned;
        for i in 0..remainder {
            assignments[i as usize] += 1;
        }

        assignments
    }
}
