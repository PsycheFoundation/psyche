use crate::{Committee, CommitteeSelection, Coordinator, Round};

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
        current_index = end_index + 1;
    }

    msg!("[assign_data_for_state] assignments: {:?}", assignments);
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
        let node_size: u64 = base_size + if i < remainder { 1 } else { 0 };

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
