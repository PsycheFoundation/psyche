use crate::{Finished, TrainingResult, fetch_data::BatchIdSet};

use super::types::PayloadState;
use psyche_coordinator::{
    committee_selection::CommitteeSelection,
    coordinator::WitnessBloom,
    node_identity::NodeIdentity,
    types::{CommitteeProof, WitnessProof},
};
use psyche_core::{BatchId, Commitment};
use psyche_modeling::DistroResult;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

pub struct RoundState {
    pub height: u32,
    pub step: u32,
    pub sent_witness: bool,
    pub sent_finished: bool,
    pub downloads: Arc<Mutex<HashMap<psyche_network::Hash, PayloadState>>>,
    #[allow(clippy::type_complexity)]
    pub results: HashMap<BatchId, Vec<(NodeIdentity, (Commitment, TrainingResult))>>,
    pub clients_finished: HashMap<NodeIdentity, Finished>,
    pub data_assignments: BTreeMap<BatchId, NodeIdentity>,
    pub blooms: Arc<Mutex<Option<(WitnessBloom, WitnessBloom)>>>,
    pub broadcasts: Vec<[u8; 32]>,
    pub committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    pub batch_ids_not_yet_trained_on: Arc<Mutex<Option<BatchIdSet>>>,
    pub self_distro_results: Vec<Vec<DistroResult>>,
}

impl RoundState {
    pub fn new() -> Self {
        Self {
            height: 0,
            step: 0,
            sent_witness: false,
            sent_finished: false,
            downloads: Arc::new(Mutex::new(HashMap::new())),
            results: HashMap::new(),
            broadcasts: Vec::new(),
            clients_finished: HashMap::new(),
            data_assignments: BTreeMap::new(),
            blooms: Arc::new(Mutex::new(None)),
            committee_info: None,
            batch_ids_not_yet_trained_on: Arc::new(Mutex::new(None)),
            self_distro_results: vec![],
        }
    }

    pub fn distro_result_blob_downloaded(&self, hash: &psyche_network::Hash) -> bool {
        self.downloads.lock().unwrap().contains_key(hash)
    }
}

impl Default for RoundState {
    fn default() -> Self {
        RoundState::new()
    }
}
