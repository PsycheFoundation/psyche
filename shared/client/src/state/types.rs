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

#[derive(Debug, Clone)]
pub enum UploadInfo {
    Hub(HubUploadInfo),
    Gcs(GcsUploadInfo),
    Dummy(),
}

impl UploadInfo {
    /// Validates that the read and write credentials are valid.
    pub async fn validate_credentials(&self) -> anyhow::Result<()> {
        match self {
            UploadInfo::Hub(HubUploadInfo { hub_token, .. }) => {
                let _api = hf_hub::api::tokio::ApiBuilder::new()
                    .with_token(Some(hub_token.clone()))
                    .build()?;
                Ok(())
            }
            UploadInfo::Gcs(_) => {
                let _storage = Storage::builder()
                    .build()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create GCS client: {}", e))?;

                let _storage_control = StorageControl::builder()
                    .build()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create GCS control client: {}", e))?;
                Ok(())
            }
            UploadInfo::Dummy() => Ok(()),
        }
    }

    /// Validates GCS bucket permissions by testing IAM permissions.
    /// Only applicable for GCS uploads; returns Ok for other upload types.
    pub async fn validate_gcs_bucket_permissions(&self) -> anyhow::Result<()> {
        if let UploadInfo::Gcs(GcsUploadInfo { gcs_bucket, .. }) = self {
            let client = StorageControl::builder().build().await?;

            let permissions_to_test = vec![
                "storage.objects.list",
                "storage.objects.get",
                "storage.objects.create",
                "storage.objects.delete",
            ];

            let resource = format!("projects/_/buckets/{}", gcs_bucket);
            let perms_vec: Vec<String> =
                permissions_to_test.iter().map(|s| s.to_string()).collect();
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
                    gcs_bucket
                )
            }
        }
        Ok(())
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

impl CheckpointConfig {
    pub fn dummy() -> Self {
        Self {
            checkpoint_dir: PathBuf::from("./checkpoints"),
            delete_old_steps: false,
            keep_steps: 1,
            hub_token: None,
            skip_upload: false,
        }
    }
}

#[derive(Debug)]
pub enum PayloadState<T: NodeIdentity> {
    Downloading((T, BatchId, BlobTicket)),
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
