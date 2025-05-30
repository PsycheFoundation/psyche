use crate::{Client, Committee, CommitteeSelection, Coordinator, Round};

use anchor_lang::prelude::msg;
use psyche_core::{BatchId, ClosedInterval, NodeIdentity, deterministic_shuffle};
use std::{collections::BTreeMap, fmt};

struct BatchAssignment<T> {
    batch_id: BatchId,
    node_id: T,
}

/// Assigns data batches to nodes based on committee roles.  
pub fn assign_data_for_state<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> BTreeMap<BatchId, T> {
    let round = coordinator.current_round().unwrap();
    let assignments = calculate_batch_assignments(coordinator, committee_selection, round);

    let mut result = BTreeMap::new();
    for assignment in assignments {
        result.insert(assignment.batch_id, assignment.node_id);
    }

    msg!("[assign_data_for_state] assignments: {:?}", result);
    result
}

pub fn get_batch_ids_for_round<T: NodeIdentity>(
    round: &Round,
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> Vec<BatchId> {
    calculate_batch_assignments(coordinator, committee_selection, round)
        .into_iter()
        .map(|assignment| assignment.batch_id)
        .collect()
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

fn calculate_batch_assignments<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
    round: &Round,
) -> Vec<BatchAssignment<T>> {
    let mut trainer_nodes = collect_trainer_nodes(coordinator, committee_selection, round);
    deterministic_shuffle(&mut trainer_nodes, round.random_seed);

    let mut assignments = Vec::with_capacity(trainer_nodes.len());
    let mut current_index = round.data_index;

    for (_, client) in trainer_nodes {
        let assigned_batch_size = client.assigned_batch_size.max(1) as u64;
        let end_index = current_index + assigned_batch_size - 1;

        assignments.push(BatchAssignment {
            batch_id: BatchId(ClosedInterval::new(current_index, end_index)),
            node_id: client.id,
        });
        current_index = end_index + 1;
    }

    assignments
}

fn collect_trainer_nodes<'a, T: NodeIdentity>(
    coordinator: &'a Coordinator<T>,
    committee_selection: &'a CommitteeSelection,
    round: &'a Round,
) -> Vec<(usize, &'a Client<T>)> {
    (0..coordinator.epoch_state.clients.len())
        .filter_map(|i| {
            let client = &coordinator.epoch_state.clients[i];
            let committee = committee_selection.get_committee(i as u64).committee;
            match committee {
                Committee::Trainer => Some((i, client)),
                Committee::TieBreaker => {
                    assert_eq!(round.tie_breaker_tasks, 0);
                    None
                }
                Committee::Verifier => {
                    assert_eq!(coordinator.config.verification_percent, 0);
                    None
                }
            }
        })
        .collect()
}
