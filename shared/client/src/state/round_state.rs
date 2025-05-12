use crate::{fetch_data::BatchIdSet, Finished, TrainingResult};

use psyche_coordinator::{
    Commitment, CommitteeProof, CommitteeSelection, WitnessBloom, WitnessProof,
    SOLANA_MAX_NUM_CLIENTS,
};
use psyche_core::{BatchId, FixedVec, NodeIdentity};
use psyche_modeling::DistroResult;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::sync::Mutex;

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
    pub batch_ids_not_yet_trained_on: Option<(usize, Arc<Mutex<BatchIdSet>>)>,
    pub self_distro_results: Vec<Vec<DistroResult>>,
    pub client_times: FixedVec<u16, SOLANA_MAX_NUM_CLIENTS>,
    pub training_started_at: Option<u64>,
}

impl<T: NodeIdentity> RoundState<T> {
    pub fn new() -> Self {
        let mut client_times = FixedVec::new();
        client_times.fill(0_u16);
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
            client_times,
            training_started_at: None,
        }
    }
}

impl<T: NodeIdentity> Default for RoundState<T> {
    fn default() -> Self {
        RoundState::new()
    }
}
