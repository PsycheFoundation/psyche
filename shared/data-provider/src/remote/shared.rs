use crate::TokenizedData;

use psyche_core::BatchId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerToClientMessage {
    TrainingData {
        data_ids: BatchId,
        raw_data: Vec<TokenizedData>,
    },
    RequestRejected {
        data_ids: BatchId,
        reason: RejectionReason,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RejectionReason {
    NotInThisRound,
    WrongDataIdForStep,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientToServerMessage {
    RequestTrainingData { data_ids: BatchId },
}
