use std::path::PathBuf;

use google_cloud_storage::client::{Storage, StorageControl};
use psyche_coordinator::CommitteeProof;
use psyche_core::{BatchId, MerkleRoot, NodeIdentity};
use psyche_data_provider::{GcsUploadInfo, HubUploadInfo};
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
    Gcs(GcsUploadInfo),
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

    /// Creates a new GCS uploader after validating bucket permissions.
    pub async fn new_gcs(bucket: String, prefix: Option<String>) -> anyhow::Result<Self> {
        let _storage = Storage::builder()
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create GCS client: {}", e))?;

        let client = StorageControl::builder()
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create GCS control client: {}", e))?;

        let permissions_to_test = vec![
            "storage.objects.list",
            "storage.objects.get",
            "storage.objects.create",
            "storage.objects.delete",
        ];

        let resource = format!("projects/_/buckets/{}", bucket);
        let perms_vec: Vec<String> = permissions_to_test.iter().map(|s| s.to_string()).collect();
        let response = client
            .test_iam_permissions()
            .set_resource(&resource)
            .set_permissions(perms_vec)
            .send()
            .await?;

        let correct_permissions = permissions_to_test
            .into_iter()
            .all(|p| response.permissions.contains(&p.to_string()));
        if !correct_permissions {
            anyhow::bail!(
                "GCS bucket {} does not have the required permissions for checkpoint upload. Make sure to set GOOGLE_APPLICATION_CREDENTIALS environment variable correctly and have the correct permissions to the bucket.",
                bucket
            )
        }

        Ok(Self::Gcs(GcsUploadInfo {
            gcs_bucket: bucket,
            gcs_prefix: prefix,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub checkpoint_dir: PathBuf,
    pub delete_old_steps: bool,
    pub keep_steps: u32,
    pub hub_token: Option<String>,
    /// Skip saving and uploading checkpoints (for testing).
    pub skip_upload: bool,
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
