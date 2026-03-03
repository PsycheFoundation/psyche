use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

const DEFAULT_RUN_DOWN_BASE_URL: &str = "https://run-down.nousresearch.com/v1";

fn base_url() -> String {
    std::env::var("RUN_DOWN_URL").unwrap_or_else(|_| DEFAULT_RUN_DOWN_BASE_URL.to_string())
}

/// Client for the Nous run-down service that provides signed URLs for GCS checkpoint
/// upload/download. Uses a generic signing function to decouple from specific wallet
/// implementations.
type SignFn = dyn Fn(&[u8]) -> Vec<u8> + Send + Sync;

pub struct RunDownClient {
    http: reqwest::Client,
    run_id: String,
    sign_fn: Arc<SignFn>,
}

impl std::fmt::Debug for RunDownClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunDownClient")
            .field("run_id", &self.run_id)
            .finish()
    }
}

impl RunDownClient {
    pub fn new(run_id: String, sign_fn: impl Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static) -> Self {
        Self {
            http: reqwest::Client::new(),
            run_id,
            sign_fn: Arc::new(sign_fn),
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    fn generate_signature(&self, expires_in_seconds: u64, nonce: u64) -> String {
        let message = format!(
            "nous-run-down-service:{}:{}:{}",
            self.run_id, expires_in_seconds, nonce
        );
        let signature_bytes = (self.sign_fn)(message.as_bytes());
        bs58::encode(&signature_bytes).into_string()
    }

    fn nonce() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Get a signed upload URL for a single file.
    pub async fn get_upload_url(&self, filename: &str) -> Result<UploadUrlResponse, RunDownError> {
        let expires_in_seconds = 3600;
        let nonce = Self::nonce();
        let signature = self.generate_signature(expires_in_seconds, nonce);

        let url = format!("{}/upload/{}", base_url(), self.run_id);
        let body = serde_json::json!({
            "filename": filename,
            "expiresInSeconds": expires_in_seconds,
            "nonce": nonce.to_string(),
        });

        info!(filename, url, "Requesting signed upload URL from run-down");

        let response = self
            .http
            .post(&url)
            .header("X-Solana-Signature", &signature)
            .json(&body)
            .send()
            .await
            .map_err(|e| RunDownError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(RunDownError::Api(format!(
                "Upload URL request failed with status {}: {}",
                status, error_text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| RunDownError::Parse(e.to_string()))
    }

    /// Get signed download URLs for all files in the run.
    pub async fn get_download_urls(&self) -> Result<DownloadUrlsResponse, RunDownError> {
        let expires_in_seconds = 3600;
        let nonce = Self::nonce();
        let signature = self.generate_signature(expires_in_seconds, nonce);

        let url = format!("{}/download/{}", base_url(), self.run_id);
        let body = serde_json::json!({
            "expiresInSeconds": expires_in_seconds,
            "nonce": nonce.to_string(),
        });

        info!(url, "Requesting signed download URLs from run-down");

        let response = self
            .http
            .post(&url)
            .header("X-Solana-Signature", &signature)
            .json(&body)
            .send()
            .await
            .map_err(|e| RunDownError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(RunDownError::Api(format!(
                "Download URLs request failed with status {}: {}",
                status, error_text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| RunDownError::Parse(e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadUrlResponse {
    pub url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadUrlEntry {
    pub path: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadUrlsResponse {
    pub urls: Vec<DownloadUrlEntry>,
    pub expires_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RunDownError {
    #[error("run-down request failed: {0}")]
    Request(String),

    #[error("run-down API error: {0}")]
    Api(String),

    #[error("failed to parse run-down response: {0}")]
    Parse(String),
}
