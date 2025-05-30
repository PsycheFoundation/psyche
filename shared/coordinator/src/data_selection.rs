use crate::{Committee, CommitteeSelection, Coordinator, Round};

use psyche_core::{BatchId, ClosedInterval, NodeIdentity, deterministic_shuffle};
use std::{collections::BTreeMap, fmt};

/// Assigns data batches to nodes based on committee roles.
pub fn assign_data_for_state<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> BTreeMap<BatchId, T> {
    let round = coordinator.current_round().unwrap();

    let trainer_nodes: Vec<_> = (0..coordinator.epoch_state.clients.len())
        .filter_map(|i| {
            let client = &coordinator.epoch_state.clients[i];
            let committee = committee_selection.get_committee(i as u64).committee;

            if matches!(committee, Committee::Trainer) {
                Some(client)
            } else {
                match committee {
                    Committee::TieBreaker => assert_eq!(round.tie_breaker_tasks, 0), // TODO
                    Committee::Verifier => assert_eq!(coordinator.config.verification_percent, 0), // TODO
                    _ => {}
                }
                None
            }
        })
        .collect();

    if trainer_nodes.is_empty() {
        return BTreeMap::new();
    }

    let batch_sizes_per_client = calculate_batch_sizes_per_client(coordinator);
    dbg!("batch_sizes_per_client: {:?}", batch_sizes_per_client);

    let mut trainer_nodes = trainer_nodes;
    deterministic_shuffle(&mut trainer_nodes, round.random_seed);

    let total_size = coordinator.get_target_global_batch_size(coordinator.current_round()) as u64;
    let num_trainers = trainer_nodes.len() as u64;
    let base_size = total_size / num_trainers;
    let remainder = total_size % num_trainers;

    let mut assignments = BTreeMap::new();
    let mut current_index = round.data_index;

    for (i, node) in trainer_nodes.iter().enumerate() {
        let node_batch_size = base_size + if (i as u64) < remainder { 1 } else { 0 };

        if node_batch_size > 0 {
            let end_index = current_index + node_batch_size - 1;
            assignments.insert(
                BatchId(ClosedInterval::new(current_index, end_index)),
                node.id,
            );
            current_index = end_index + 1;
        }
    }

    assignments
}

pub fn get_batch_ids_for_round<T: NodeIdentity>(
    round: &Round,
    coordinator: &Coordinator<T>,
    num_trainer_nodes: u64,
) -> Vec<BatchId> {
    let start = round.data_index;
    let total_size = coordinator.get_target_global_batch_size(Some(round)) as u64;
    let end = start + total_size;

    let base_size = total_size / num_trainer_nodes;
    let remainder = total_size % num_trainer_nodes;

    let mut batch_ids = Vec::with_capacity(num_trainer_nodes as usize);
    let mut current = start;

    for i in 0..num_trainer_nodes {
        let node_size = base_size + if i < remainder { 1 } else { 0 };

        if node_size > 0 {
            let batch_end = current + node_size - 1;
            batch_ids.push(BatchId(ClosedInterval::new(current, batch_end)));
            current = batch_end + 1;

            if current >= end {
                break;
            }
        }
    }

    batch_ids
}

/// Retrieves all batch IDs assigned to a specific node from an interval tree, converting data indices to batches.
pub fn get_batch_ids_for_node<V: fmt::Display + Eq + std::hash::Hash>(
    tree: &BTreeMap<BatchId, V>,
    node_identity: &V,
) -> Vec<BatchId> {
    tree.iter()
        .filter_map(|(interval, assigned_node)| {
            if assigned_node == node_identity {
                Some(*interval)
            } else {
                None
            }
        })
        .collect()
}

pub fn get_data_index_for_step<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    target_step: u32,
) -> u64 {
    if target_step <= 1 || target_step > coordinator.config.total_steps {
        return 0;
    }

    let mut current_data_index: u64 = 0;
    let max_seq_len = coordinator.get_sequence_length() as u64;

    for _ in 1..target_step {
        let tokens_processed_before_step = current_data_index * max_seq_len;

        let batch_size_for_step = coordinator
            .config
            .get_batch_size(tokens_processed_before_step) as u64;

        current_data_index += batch_size_for_step;
    }

    current_data_index
}

fn calculate_batch_sizes_per_client<T: NodeIdentity>(coordinator: &Coordinator<T>) -> Vec<u64> {
    let mut scores_per_node = Vec::<f64>::new();

    for (client_index, time) in coordinator.client_training_times.iter().enumerate() {
        let score = 1.0 / (*time as f64); // We had time/batch, now we have batch/time
        scores_per_node.push(score);
    }

    // Initialize final assignments
    let n_clients = coordinator.epoch_state.clients.len();
    let mut final_assignments = vec![0u64; n_clients];

    // First pass: assign 1 batch to each node with non-zero score
    let mut remaining_batches = coordinator.config.global_batch_size_end as u64;
    for (i, &score) in scores_per_node.iter().take(n_clients).enumerate() {
        if score > 0.0 {
            final_assignments[i] = 1;
            remaining_batches -= 1;
        }
    }

    // If nothing is remaining then we are done
    if remaining_batches == 0 {
        return final_assignments;
    }

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
        .map(|&x| x.floor() as u64)
        .collect::<Vec<u64>>();

    // Calculate how many batches are still unassigned
    let assigned: u64 = additional.iter().sum();
    let still_remaining = remaining_batches - assigned;

    // Distribute remaining batches by fractional part
    if still_remaining == 0 {
        // If no batches are left, we can return the final assignments
        return final_assignments;
    }

    let mut fractional: Vec<(usize, f64)> = raw_remaining
        .iter()
        .enumerate()
        .map(|(i, &x)| (i, x - x.floor()))
        .collect();

    fractional.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (idx, _) in fractional.iter().take(still_remaining as usize) {
        additional[*idx] += 1;
    }

    // Add additional assignments to base assignments
    for (base, extra) in final_assignments.iter_mut().zip(additional.iter()) {
        *base += *extra;
    }

    final_assignments
}
