use anyhow::Result;
use psyche_coordinator::{Coordinator, HealthChecks, Witness, WitnessMetadata};
use psyche_core::NodeIdentity;
use serde::{Deserialize, Serialize};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpportunisticData {
    WitnessStep(Witness, WitnessMetadata),
    WarmupStep(Witness),
    CooldownStep(Witness),
}

impl OpportunisticData {
    pub fn kind(&self) -> &'static str {
        match self {
            OpportunisticData::WitnessStep(..) => "witness",
            OpportunisticData::WarmupStep(..) => "warmup",
            OpportunisticData::CooldownStep(..) => "cooldown",
        }
    }
}

#[async_trait::async_trait]
pub trait Backend<T: NodeIdentity>: Send + Sync {
    /// # Cancel safety
    ///
    /// This method must be cancel safe.
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<T>>;
    async fn send_witness(&mut self, opportunistic_data: OpportunisticData) -> Result<()>;
    async fn send_health_check(&mut self, health_check: HealthChecks<T>) -> Result<()>;
    async fn send_checkpoint(
        &mut self,
        checkpoint: psyche_coordinator::model::Checkpoint,
    ) -> Result<()>;
}
