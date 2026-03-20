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

/// Validated checkpoint uploader. Can only be constructed via async methods
/// that validate credentials and permissions on creation.
#[derive(Debug, Clone)]
pub enum CheckpointUploader {
    Hub(HubUploadInfo),
    Gcs(Arc<RunDownClient>),
    Dummy,
}

impl CheckpointUploader {
    /// Creates a new HF Hub uploader after validating write permissions to the repo.
    pub async fn new_hub(repo: String, token: String) -> anyhow::Result<Self> {
        let api = hf_hub::api::tokio::ApiBuilder::new()
            .with_token(Some(token.clone()))
            .build()?;
        let api_repo = api.repo(hf_hub::Repo::model(repo.clone()));
        if !api_repo.is_writable().await {
            anyhow::bail!(
                "Checkpoint upload repo {} is not writable with the provided HF token.",
                repo
            );
        }
        Ok(Self::Hub(HubUploadInfo {
            hub_repo: repo,
            hub_token: token,
        }))
    }

    /// Creates a new GCS uploader using run-down signed URLs.
    /// Auth is validated at upload time via signed URLs.
    pub fn new_gcs(run_down_client: Arc<RunDownClient>) -> Self {
        Self::Gcs(run_down_client)
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
