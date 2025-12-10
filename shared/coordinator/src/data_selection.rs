use crate::{Committee, CommitteeSelection, Coordinator, Round};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Client, ClientState, CommitteeSelection, Coordinator};
    use bytemuck::Zeroable;
    use psyche_core::{FixedVec, NodeIdentity};

    #[derive(
        serde::Serialize,
        serde::Deserialize,
        Clone,
        Debug,
        Hash,
        PartialEq,
        Eq,
        Default,
        Copy,
        bytemuck::Zeroable,
    )]
    struct TestNode(u64);

    impl NodeIdentity for TestNode {
        fn get_p2p_public_key(&self) -> &[u8; 32] {
            todo!()
        }
    }

    impl fmt::Display for TestNode {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Node({})", self.0)
        }
    }

    impl anchor_lang::AnchorSerialize for TestNode {
        fn serialize<W: std::io::Write>(&self, _: &mut W) -> std::io::Result<()> {
            unimplemented!()
        }
    }

    impl anchor_lang::AnchorDeserialize for TestNode {
        fn deserialize_reader<R: std::io::Read>(_: &mut R) -> std::io::Result<Self> {
            unimplemented!()
        }
    }

    impl anchor_lang::Space for TestNode {
        const INIT_SPACE: usize = 0;
    }

    impl AsRef<[u8]> for TestNode {
        fn as_ref(&self) -> &[u8] {
            todo!()
        }
    }

    fn create_test_coordinator(
        num_nodes: usize,
        global_batch_size: u16,
        total_steps: u32,
    ) -> Coordinator<TestNode> {
        let clients: Vec<_> = (0..num_nodes)
            .map(|i| Client {
                id: TestNode(i as u64),
                state: ClientState::Healthy,
                exited_height: 0,
            })
            .collect();

        let mut coordinator = Coordinator::<TestNode>::zeroed();
        coordinator.config.total_steps = total_steps;
        coordinator.config.global_batch_size_start = global_batch_size;
        coordinator.config.global_batch_size_end = global_batch_size;
        coordinator.epoch_state.clients = FixedVec::from_iter(clients);

        coordinator.current_round_mut().unwrap().clients_len =
            coordinator.epoch_state.clients.len() as u16;

        coordinator
    }

    #[test]
    fn test_even_distribution() {
        // 4 trainers, global batch size 100 -> each gets 25
        let coordinator = create_test_coordinator(4, 100, 10);

        let assignments = assign_data_for_state(
            &coordinator,
            &CommitteeSelection::from_coordinator(&coordinator, 0).unwrap(),
        );
        assert_eq!(assignments.len(), 4);

        for batch_id in assignments.keys() {
            let size = batch_id.0.end - batch_id.0.start + 1;
            assert_eq!(size, 25);
        }

        let total_assigned: u64 = assignments.keys().map(|b| b.0.end - b.0.start + 1).sum();
        assert_eq!(total_assigned, 100);
    }

    #[test]
    fn test_uneven_distribution_with_remainder() {
        // 24 trainers, global batch size 384
        let coordinator = create_test_coordinator(23, 384, 10);

        let assignments = assign_data_for_state(
            &coordinator,
            &CommitteeSelection::from_coordinator(&coordinator, 0).unwrap(),
        );
        assert_eq!(assignments.len(), 23);

        let mut sizes: Vec<u64> = assignments
            .keys()
            .map(|b| b.0.end - b.0.start + 1)
            .collect();
        sizes.sort();

        let mut expected = [16; 7].to_vec();
        expected.extend([17; 16]);
        assert_eq!(sizes, expected);

        let total: u64 = sizes.iter().sum();
        assert_eq!(total, 384);
    }

    #[test]
    fn test_larger_remainder() {
        // 5 trainers, global batch size 13 -> remainder of 3
        // Expected: base_size=2, so 3 nodes get 3, 2 nodes get 2
        let coordinator = create_test_coordinator(5, 13, 10);

        let assignments = assign_data_for_state(
            &coordinator,
            &CommitteeSelection::from_coordinator(&coordinator, 0).unwrap(),
        );
        assert_eq!(assignments.len(), 5);

        let mut sizes: Vec<u64> = assignments
            .keys()
            .map(|b| b.0.end - b.0.start + 1)
            .collect();
        sizes.sort();

        // Base: 13/5 = 2, remainder: 13%5 = 3
        // First 3 nodes get 3, last 2 get 2
        assert_eq!(sizes, vec![2, 2, 3, 3, 3]);

        let total: u64 = sizes.iter().sum();
        assert_eq!(total, 13);
    }
}
