use crate::{fetch_data::BatchIdSet, Finished, TrainingResult};

use psyche_coordinator::{
    Commitment, CommitteeProof, CommitteeSelection, WitnessBloom, WitnessProof,
};
use psyche_core::{BatchId, NodeIdentity};
use psyche_modeling::DistroResult;
use std::collections::{BTreeMap, HashMap};

use super::types::PayloadState;

pub struct RoundState<T: NodeIdentity> {
    pub height: u32,
    pub step: u32,
    pub sent_witness: bool,
    pub sent_finished: bool,
    pub downloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    #[allow(clippy::type_complexity)]
    pub results: HashMap<BatchId, Vec<(T, (Commitment, TrainingResult))>>,
    pub clients_finished: HashMap<T, Finished>,
    pub data_assignments: BTreeMap<BatchId, T>,
    pub blooms: Option<(WitnessBloom, WitnessBloom)>,
    pub broadcasts: Vec<[u8; 32]>,
    pub committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    pub batch_ids_not_yet_trained_on: Option<(usize, BatchIdSet)>,
    pub self_distro_results: Vec<Vec<DistroResult>>,
}

impl<T: NodeIdentity> RoundState<T> {
    pub fn new() -> Self {
        Self {
            height: 0,
            step: 0,
            sent_witness: false,
            sent_finished: false,
            downloads: HashMap::new(),
            results: HashMap::new(),
            broadcasts: Vec::new(),
            clients_finished: HashMap::new(),
            data_assignments: BTreeMap::new(),
            blooms: None,
            committee_info: None,
            batch_ids_not_yet_trained_on: None,
            self_distro_results: vec![],
        }
    }
}

impl<T: NodeIdentity> Default for RoundState<T> {
    fn default() -> Self {
        RoundState::new()
    }
}
