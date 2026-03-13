use std::path::PathBuf;
use std::sync::Arc;

use psyche_coordinator::CommitteeProof;
use psyche_core::{BatchId, MerkleRoot, NodeIdentity};
use psyche_data_provider::{HubUploadInfo, RunDownClient};
use psyche_modeling::DistroResult;
use psyche_network::{BlobTicket, TransmittableDistroResult};
use tch::TchError;
use thiserror::Error;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub enum UploadInfo {
    Hub(HubUploadInfo),
    Gcs(Arc<RunDownClient>),
    Dummy(),
}

#[derive(Debug, Clone)]
pub enum UploadCredentials {
    HubToken(String),
    Skip,
}

impl UploadCredentials {
    pub async fn validate(&self) -> anyhow::Result<()> {
        match self {
            UploadCredentials::HubToken(token) => {
                let _api = hf_hub::api::tokio::ApiBuilder::new()
                    .with_token(Some(token.clone()))
                    .build()?;
                Ok(())
            }
            UploadCredentials::Skip => Ok(()),
        }
    }
}

impl From<&UploadInfo> for UploadCredentials {
    fn from(info: &UploadInfo) -> Self {
        match info {
            UploadInfo::Hub(HubUploadInfo { hub_token, .. }) => {
                UploadCredentials::HubToken(hub_token.clone())
            }
            UploadInfo::Gcs(_) | UploadInfo::Dummy() => UploadCredentials::Skip,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub checkpoint_dir: PathBuf,
    pub delete_old_steps: bool,
    pub keep_steps: u32,
    pub hub_token: Option<String>,
    pub skip_upload: bool,
    pub run_down_client: Option<Arc<RunDownClient>>,
}

impl CheckpointConfig {
    pub fn dummy() -> Self {
        Self {
            checkpoint_dir: PathBuf::from("./checkpoints"),
            delete_old_steps: false,
            keep_steps: 1,
            hub_token: None,
            skip_upload: false,
            run_down_client: None,
        }
    }
}

#[derive(Debug)]
pub enum PayloadState {
    Downloading((NodeIdentity, BatchId, BlobTicket)),
    Deserializing(JoinHandle<Result<(Vec<DistroResult>, u32), DeserializeError>>),
}

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("Deserialize thread crashed")]
    DeserializeThreadCrashed,

    #[error("Deserialize error: {0}")]
    Deserialize(#[from] TchError),
}

pub struct DistroBroadcastAndPayload {
    pub step: u32,
    pub batch_id: BatchId,
    pub commitment_data_hash: [u8; 32],
    pub proof: CommitteeProof,
    pub distro_result: TransmittableDistroResult,
    pub original_distro_result: Vec<DistroResult>,
}

pub struct FinishedBroadcast {
    pub step: u32,
    pub merkle: MerkleRoot,
    pub commitment_data_hash: [u8; 32],
    pub proof: CommitteeProof,
    pub warmup: bool,
}
