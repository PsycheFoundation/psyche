use anyhow::{Context, Result, bail};
use psyche_solana_rpc::SolanaBackend;
use serde::{Deserialize, Serialize};

const RUN_DOWN_SERVICE_BASE_URL: &str = "https://run-down.nousresearch.com/v1";

/// Generate a signed message for the Nous run-down service API
///
/// Creates a message in the format: `nous-run-down-service:{run_id}:{expires_in_seconds}:{nonce}`
/// and signs it using the provided backend's wallet.
///
/// Returns the base58-encoded signature.
pub fn generate_signature(
    backend: &SolanaBackend,
    run_id: &str,
    expires_in_seconds: u64,
    nonce: u64,
) -> String {
    let message = format!(
        "nous-run-down-service:{}:{}:{}",
        run_id, expires_in_seconds, nonce
    );
    let message_bytes = message.as_bytes();
    let signature = backend.sign_message(message_bytes);
    bs58::encode(&signature).into_string()
}

/// Make a signed POST request to the run-down service API
async fn make_signed_request(
    client: &reqwest::Client,
    url: &str,
    signature: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    let response = client
        .post(url)
        .header("X-Solana-Signature", signature)
        .json(&body)
        .send()
        .await
        .context("Failed to send request to run-down service")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        bail!("API request failed with status {}: {}", status, error_text);
    }

    response
        .json()
        .await
        .context("Failed to parse response JSON")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadUrlResponse {
    pub url: String,
    pub expires_at: String,
}

/// Get a signed upload URL for a file
pub async fn get_upload_url(
    client: &reqwest::Client,
    run_id: &str,
    signature: &str,
    filename: &str,
    expires_in_seconds: u64,
    nonce: u64,
) -> Result<UploadUrlResponse> {
    let url = format!("{}/upload/{}", RUN_DOWN_SERVICE_BASE_URL, run_id);

    let body = serde_json::json!({
        "filename": filename,
        "expiresInSeconds": expires_in_seconds,
        "nonce": nonce.to_string(),
    });

    let response_value = make_signed_request(client, &url, signature, body).await?;

    serde_json::from_value(response_value).context("Failed to parse upload URL response")
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

/// Get signed download URLs for all files in a run
pub async fn get_download_urls(
    client: &reqwest::Client,
    run_id: &str,
    signature: &str,
    expires_in_seconds: u64,
    nonce: u64,
) -> Result<DownloadUrlsResponse> {
    let url = format!("{}/download/{}", RUN_DOWN_SERVICE_BASE_URL, run_id);

    let body = serde_json::json!({
        "expiresInSeconds": expires_in_seconds,
        "nonce": nonce.to_string(),
    });

    let response_value = make_signed_request(client, &url, signature, body).await?;

    serde_json::from_value(response_value).context("Failed to parse download URLs response")
}
