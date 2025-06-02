use crate::{Committee, CommitteeSelection, Coordinator, Round};

use anchor_lang::prelude::msg;
use psyche_core::{deterministic_shuffle, BatchId, ClosedInterval, NodeIdentity};
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
                Some((i, client))
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

    let mut trainer_nodes = trainer_nodes;
    deterministic_shuffle(&mut trainer_nodes, round.random_seed);

    let mut assignments = BTreeMap::new();
    let mut current_index = round.data_index;
    let assigned_batch_sizes = calculate_batch_sizes_per_client(coordinator);
    msg!(
        "[assign_data_for_state] calculated assigned batch sizes: {:?}",
        assigned_batch_sizes
    );

    // Use assigned batch sizes for batch size assignments
    for (client_index, node) in trainer_nodes {
        // We don't want nodes not training so if there was no batch size assigned (or calculated as 0),
        // we assign 1 at least.
        let node_batch_size = assigned_batch_sizes
            .get(client_index)
            .cloned()
            .unwrap_or(1u64);

        let end_index = current_index + node_batch_size - 1;
        assignments.insert(
            BatchId(ClosedInterval::new(current_index, end_index)),
            node.id,
        );
        current_index = end_index + 1;
    }

    msg!("[assign_data_for_state] assignments: {:?}", assignments);
    assignments
}

pub fn get_batch_ids_for_round<T: NodeIdentity>(
    round: &Round,
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> Vec<BatchId> {
    // Get the list of trainer nodes and their original indices, similar to assign_data_for_state
    // The elements are (original_client_index, reference_to_client_data)
    let mut trainer_info: Vec<_> = (0..coordinator.epoch_state.clients.len())
        .filter_map(|i| {
            let client = &coordinator.epoch_state.clients[i];
            let proof = committee_selection.get_committee(i as u64);
            if matches!(proof.committee, Committee::Trainer) {
                Some((i, client))
            } else {
                // If it's not a trainer then there's nothing to check as there's no batch assignment for it.
                None
            }
        })
        .collect();

    // Apply the same deterministic shuffle as in assign_data_for_state
    // This ensures the batch IDs are generated in the same order as assignments are made.
    deterministic_shuffle(&mut trainer_info, round.random_seed);

    let mut batch_ids = Vec::with_capacity(trainer_info.len());
    let mut current_data_idx = round.data_index;

    // TODO(dy) do we really need to recalculate here?
    let assigned_batch_sizes = calculate_batch_sizes_per_client(coordinator);

    for (client_original_idx, _client_ref) in &trainer_info {
        let node_batch_actual_size = assigned_batch_sizes
            .get(*client_original_idx)
            .cloned()
            .unwrap_or(1u64); // Default to batch size of 1 if for some reason it's not found

        let end_data_idx = current_data_idx + node_batch_actual_size - 1;
        batch_ids.push(BatchId(ClosedInterval::new(current_data_idx, end_data_idx)));
        current_data_idx = end_data_idx + 1; // Move to the start of the next batch
    }

    msg!("[get_batch_ids_for_round] batch_ids: {:?}", batch_ids);
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
    let n_clients = coordinator.epoch_state.clients.len();
    if n_clients == 0 {
        return Vec::new();
    }

    // Calculate scores for each client.
    // A score of 0.0 is assigned if training time is not positive.
    let mut client_scores = Vec::with_capacity(n_clients);
    for i in 0..n_clients {
        let score = match coordinator.client_training_times.get(i) {
            Some(time_ref) => {
                // Assuming time_ref points to a numeric type that can be cast to f64.
                // The original code used `*time as f64`.
                let time_val_f64 = *time_ref as f64;
                if time_val_f64 > 0.0 {
                    1.0 / time_val_f64
                } else {
                    0.0 // Time is zero, negative, or otherwise non-positive.
                }
            }
            None => 0.0, // No training time recorded for this client.
        };
        client_scores.push(score);
    }
    msg!(
        "[calculate_batch_sizes_per_client] Initial client scores: {:?}",
        client_scores
    );

    let mut final_assignments = vec![0u64; n_clients];
    let mut remaining_batches = coordinator.config.global_batch_size_end as u64;

    // First pass: assign 1 batch to each client with a positive score, if batches are available.
    for i in 0..n_clients {
        if remaining_batches == 0 {
            break;
        }
        if client_scores[i] > 0.0 {
            // Score is positive and finite.
            final_assignments[i] = 1;
            remaining_batches -= 1;
        }
    }
    msg!(
        "[calculate_batch_sizes_per_client] remaining_batches after first pass: {}",
        remaining_batches
    );

    if remaining_batches == 0 {
        return final_assignments;
    }

    // Collect clients eligible for further distribution (those with positive scores).
    // Stores (original_client_index, score).
    let eligible_clients_for_distribution: Vec<(usize, f64)> = client_scores
        .iter()
        .enumerate()
        .filter_map(|(idx, &score)| {
            if score > 0.0 {
                Some((idx, score))
            } else {
                None
            }
        })
        .collect();

    msg!(
        "[calculate_batch_sizes_per_client] Eligible clients for distribution: {:?}",
        eligible_clients_for_distribution
    );

    if eligible_clients_for_distribution.is_empty() {
        // No clients have positive scores. Distribute remaining_batches equally among all n_clients.
        // At this point, final_assignments are all 0s because no client got a batch in the first pass.
        msg!("[calculate_batch_sizes_per_client] No eligible clients with positive scores. Distributing {} remaining batches equally among {} clients.", remaining_batches, n_clients);
        if n_clients > 0 {
            // Should be true if we are here and remaining_batches > 0
            let batches_per_client = remaining_batches / n_clients as u64;
            let mut extra_batches_to_distribute = remaining_batches % n_clients as u64;
            for assignment in final_assignments.iter_mut().take(n_clients) {
                *assignment = batches_per_client;
                if extra_batches_to_distribute > 0 {
                    *assignment += 1;
                    extra_batches_to_distribute -= 1;
                }
            }
        }
        return final_assignments;
    }

    // Normalize scores for the eligible clients.
    let sum_eligible_scores: f64 = eligible_clients_for_distribution
        .iter()
        .map(|(_, score)| score)
        .sum();
    // sum_eligible_scores must be > 0.0 as eligible_clients_for_distribution is not empty and all scores are > 0.0.

    // This vector will store the additional batches calculated in this pass for each client.
    let mut additional_assignments = vec![0u64; n_clients];
    // Stores raw f64 batch counts for fractional part calculation, mapped by original client index.
    let mut raw_values_for_fractional = BTreeMap::new();

    for (original_idx, client_score) in &eligible_clients_for_distribution {
        let normalized_score = *client_score / sum_eligible_scores;
        let raw_assigned_batches_for_client = normalized_score * remaining_batches as f64;

        raw_values_for_fractional.insert(*original_idx, raw_assigned_batches_for_client);
        additional_assignments[*original_idx] = raw_assigned_batches_for_client.floor() as u64;
    }

    let assigned_in_normalization_pass: u64 = additional_assignments.iter().sum();
    let still_remaining_for_fractional =
        remaining_batches.saturating_sub(assigned_in_normalization_pass);

    if still_remaining_for_fractional > 0 {
        // Collect fractional parts only from eligible clients who received some raw assignment.
        let mut fractional_parts: Vec<(usize, f64)> = raw_values_for_fractional
            .iter()
            .map(|(&original_idx, &raw_val)| {
                let frac = raw_val - raw_val.floor();
                // frac should be finite if raw_val was. Handle NaN defensively.
                (original_idx, if frac.is_nan() { 0.0 } else { frac })
            })
            .collect();

        fractional_parts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (original_idx, _) in fractional_parts
            .iter()
            .take(still_remaining_for_fractional as usize)
        {
            additional_assignments[*original_idx] += 1;
        }
    }

    // Add the calculated additional assignments to the base assignments from the first pass.
    for i in 0..n_clients {
        final_assignments[i] += additional_assignments[i];
    }

    msg!(
        "[calculate_batch_sizes_per_client] Final assignments: {:?}",
        final_assignments
    );
    final_assignments
}
