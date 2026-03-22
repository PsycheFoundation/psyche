use psyche_coordinator::{
    coordinator::{Coordinator, HealthChecks},
    model,
};
use psyche_watcher::OpportunisticData;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Join { run_id: String },
    Witness(Box<OpportunisticData>),
    HealthCheck(HealthChecks),
    Checkpoint(Box<model::CheckpointBytes>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    Coordinator(Box<Coordinator>),
}
