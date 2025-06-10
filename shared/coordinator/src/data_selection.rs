use crate::{Committee, CommitteeSelection, Coordinator, Round};

use anchor_lang::prelude::msg;
use psyche_core::{BatchId, ClosedInterval, NodeIdentity, deterministic_shuffle};
use std::{collections::BTreeMap, fmt};

/// Assigns data batches to nodes based on committee roles.
pub fn assign_data_for_state<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> BTreeMap<BatchId, T> {
    let round = coordinator.current_round().unwrap();

    // Get trainer nodes references to avoid cloning
    let mut trainer_nodes: Vec<_> = coordinator
        .epoch_state
        .clients
        .iter()
        .enumerate()
        .filter_map(|(i, client)| {
            let committee = committee_selection.get_committee(i as u64).committee;

            match committee {
                Committee::Trainer => Some((client, client.id)),
                Committee::TieBreaker => {
                    assert_eq!(round.tie_breaker_tasks, 0); // TODO
                    None
                }
                Committee::Verifier => {
                    assert_eq!(coordinator.config.verification_percent, 0); // TODO
                    None
                }
            }
        })
        .collect();

    if trainer_nodes.is_empty() {
        return BTreeMap::new();
    }

    deterministic_shuffle(&mut trainer_nodes, round.random_seed);

    let mut assignments = BTreeMap::new();
    let mut current_index = round.data_index;

    // Use references in the loop to avoid moving/copying data
    for (client, node_id) in &trainer_nodes {
        let assigned_batch_size = client.assigned_batch_size.max(1) as u64;
        let end_index = current_index + assigned_batch_size - 1;

        assignments.insert(
            BatchId(ClosedInterval::new(current_index, end_index)),
            *node_id,
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

    for (client_original_idx, _client_ref) in &trainer_info {
        let client = coordinator.epoch_state.clients.get(*client_original_idx);
        let Some(client) = client else {
            continue;
        };

        let client_assigned_batch_size = client.assigned_batch_size.max(1) as u64;
        let end_data_idx = match client_assigned_batch_size {
            0 | 1 => current_data_idx,
            _ => current_data_idx + client_assigned_batch_size - 1,
        };

        batch_ids.push(BatchId(ClosedInterval::new(current_data_idx, end_data_idx)));
        current_data_idx = end_data_idx + 1; // Move to the start of the next batch
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
