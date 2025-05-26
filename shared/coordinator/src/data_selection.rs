use crate::{Client, Committee, CommitteeSelection, Coordinator, Round};

use anchor_lang::prelude::msg;
use psyche_core::{
    deterministic_shuffle, index_to_value, BatchId, ClosedInterval, NodeIdentity,
    BATCH_SIZE_INDEX_BITS,
};
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

    let max_round_batch_size = coordinator.get_target_global_batch_size(Some(round));

    // Use assigned batch sizes for batch size assignments
    for (client_index, node) in trainer_nodes {
        let batch_size_index = coordinator.epoch_state.clients[client_index].assigned_batch_size;

        let mut node_batch_size = index_to_value(
            batch_size_index,
            max_round_batch_size,
            BATCH_SIZE_INDEX_BITS,
        ) as u64;

        // We don't want nodes not training so if there was no batch size assigned (or calculated as 0),
        // we assign 1 at least.
        if node_batch_size == 0 {
            node_batch_size = 1u64;
        }

        msg!(
            "[assign_data_for_state] node: {:?}, batch_size_index: {}, batch_size: {}",
            node,
            batch_size_index,
            node_batch_size
        );

        let end_index = current_index + node_batch_size - 1;
        assignments.insert(
            BatchId(ClosedInterval::new(current_index, end_index)),
            node.id,
        );
        msg!(
            "[assign_data_for_state] assigned batch: {:?} to node: {:?}",
            BatchId(ClosedInterval::new(current_index, end_index)),
            node.id
        );
        current_index = end_index + 1;
    }

    msg!("[assign_data_for_state] assignments: {:?}", assignments);
    assignments
}

pub fn get_batch_ids_for_round<T: NodeIdentity>(
    round: &Round,
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection, // Pass CommitteeSelection to determine trainers and their order
) -> Vec<BatchId> {
    // Get the list of trainer nodes and their original indices, similar to assign_data_for_state
    // The elements are (original_client_index, reference_to_client_data)
    let mut trainer_info: Vec<(usize, &Client<T>)> = (0..coordinator.epoch_state.clients.len())
        .filter_map(|i| {
            let client = &coordinator.epoch_state.clients[i];
            let proof = committee_selection.get_committee(i as u64);
            if matches!(proof.committee, Committee::Trainer) {
                Some((i, client))
            } else {
                // This part ensures that only trainers are selected, matching assign_data_for_state's filtering.
                // The asserts in assign_data_for_state for TieBreaker/Verifier are for specific zero-config cases
                // and don't change which nodes are considered trainers for data assignment.
                None
            }
        })
        .collect();

    // Apply the same deterministic shuffle as in assign_data_for_state
    // This ensures the batch IDs are generated in the same order as assignments are made.
    deterministic_shuffle(&mut trainer_info, round.random_seed);

    let mut batch_ids = Vec::with_capacity(trainer_info.len());
    let mut current_data_idx = round.data_index;
    // This is the target total number of items for the round, used as max_value for index_to_value.
    let max_round_batch_size_val = coordinator.get_target_global_batch_size(Some(round));

    for (client_original_idx, _client_ref) in &trainer_info {
        // Get the assigned_batch_size (index) for the current trainer node
        let batch_size_idx =
            coordinator.epoch_state.clients[*client_original_idx].assigned_batch_size;

        // Convert the index to an actual number of items for this node's batch
        let mut node_batch_actual_size = index_to_value(
            batch_size_idx,
            max_round_batch_size_val,
            BATCH_SIZE_INDEX_BITS,
        ) as u64;

        // Ensure a minimum batch size of 1, consistent with assign_data_for_state
        if node_batch_actual_size == 0 {
            node_batch_actual_size = 1u64;
        }

        if node_batch_actual_size > 0 {
            let end_data_idx = current_data_idx + node_batch_actual_size - 1;
            batch_ids.push(BatchId(ClosedInterval::new(current_data_idx, end_data_idx)));
            current_data_idx = end_data_idx + 1; // Move to the start of the next batch
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
