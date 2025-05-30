use std::collections::BTreeMap;

use psyche_coordinator::{
    Client, Coordinator, Witness, WitnessMetadata, MAX_NUM_WITNESSED_CLIENTS,
    SOLANA_MAX_NUM_CLIENTS,
};
use psyche_core::{
    value_to_nearest_index, BatchId, FixedVec, MerkleRoot, MerkleTree, NodeIdentity,
    BATCH_SIZE_INDEX_BITS,
};
use psyche_watcher::OpportunisticData;
use thiserror::Error;
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};
use tracing::{debug, info, trace};

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
            self.identity,
            previous_round,
            current_round,
            &state.epoch_state.clients,
            state.config.global_batch_size_end, // TODO should we use end or start here?
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
        identity: T,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
        clients: &[Client<T>],
        global_batch_size: u16,
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
            &identity,
            &current_round.client_times,
            &current_round.data_assignments,
            clients,
            global_batch_size,
        );

        // Find which partition we are currently on (0, 1, 2, etc.). Division should be safe as it floors
        let client_index =
            Self::get_client_index(&identity, clients).expect("Client not found in clients list");
        let current_client_partition_index = client_index as usize / MAX_NUM_WITNESSED_CLIENTS;
        let slice_start_index = current_client_partition_index * MAX_NUM_WITNESSED_CLIENTS;
        // slice_end_index is not strictly needed for the new logic but good for context/debug
        let slice_end_index = slice_start_index + MAX_NUM_WITNESSED_CLIENTS;
        debug!(
            "[get_witness_to_send] Client index: {}, partition index: {}, slice start: {}, slice end: {}. Assignments_vec length: {}",
            client_index,
            current_client_partition_index,
            slice_start_index,
            slice_end_index,
            assigments_vec.len()
        );

        // A bit hackish but I'm creating a new vector with 0 padding for now as we need exact size
        // Figure out a better way to handle this later
        let mut assignments_partition_values = vec![0u8; MAX_NUM_WITNESSED_CLIENTS];
        for i in 0..MAX_NUM_WITNESSED_CLIENTS {
            let source_index = slice_start_index + i;
            if source_index < assigments_vec.len() {
                assignments_partition_values[i] = assigments_vec[source_index];
            }
        }

        let proposed_batch_sizes: FixedVec<u8, { MAX_NUM_WITNESSED_CLIENTS }> =
            FixedVec::try_from(assignments_partition_values.as_slice()).unwrap_or_else(|e| {
                // Should never be reached but just in case
                panic!(
                    "Failed to convert assignments_partition_values (length {}) to FixedVec. Error: {:?}",
                    assignments_partition_values.len(),
                    e
                );
            });

        Some(Witness {
            proof: *proof,
            participant_bloom,
            broadcast_bloom,
            broadcast_merkle,
            proposed_batch_sizes,
        })
    }

    fn calculate_assignments_given_client_times<T: NodeIdentity>(
        identity: &T,
        client_times: &FixedVec<u16, SOLANA_MAX_NUM_CLIENTS>,
        data_assignments: &BTreeMap<BatchId, T>,
        clients: &[Client<T>],
        global_batch_size: u16,
    ) -> Vec<u8> {
        let mut scores_per_node: Vec<f64> = Vec::new();
        
        // We set our client time as the average of all other client times as workaround
        // since reporting our own time will be very low
        let non_zero_times: Vec<u16> = client_times
            .iter()
            .filter(|&&time| time > 0)
            .cloned()
            .collect();
        let average_client_time = if !non_zero_times.is_empty() {
            non_zero_times.iter().sum::<u16>() / non_zero_times.len() as u16
        } else {
            0
        };
        // Assign our own time as the average of the rest
        debug!(
            "[calculate_assignments] Average client time: {}",
            average_client_time
        );

        let my_client_index =
            Self::get_client_index(identity, clients).expect("Client not found in clients list");

        for i in 0..clients.len() {
            let batches_assigned =
                Self::number_of_data_assignments_for_client(&clients[i].id, data_assignments);

            let client_time = if i != my_client_index {
                client_times[i]
            } else {
                average_client_time
            };
            let score: f64 = if client_time != 0 {
                (batches_assigned as f64) / (client_time as f64)
            } else {
                0.0
            };
            scores_per_node.push(score);

            debug!(
                "[calculate_assignments] client {} ({})\n
                client_time: {:?} , batches trained: {}, calculated score: {}\n",
                i, clients[i].id, client_times[i], batches_assigned, score,
            );
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
                fractional
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                for (idx, _) in fractional.iter().take(still_remaining as usize) {
                    additional[*idx] += 1;
                }
            }

            // Add additional assignments to base assignments
            for (base, extra) in final_assignments.iter_mut().zip(additional.iter()) {
                *base += *extra;
            }
        }

        // Clamp each assignment to the nearest value in a generated sequence
        let clamped_indices: Vec<u8> = final_assignments
            .into_iter()
            .map(|val| value_to_nearest_index(val, global_batch_size, BATCH_SIZE_INDEX_BITS))
            .collect();

        debug!("Calculated indices: {:?}", clamped_indices);
        clamped_indices
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

    fn get_client_index<T: NodeIdentity>(identity: &T, clients: &[Client<T>]) -> Option<usize> {
        clients.iter().position(|client| &client.id == identity)
    }
}
