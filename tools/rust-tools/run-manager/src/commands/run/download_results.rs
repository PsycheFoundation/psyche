use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::commands::Command;
use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadUrlEntry {
    path: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadUrlsResponse {
    urls: Vec<DownloadUrlEntry>,
    #[serde(rename = "expiresAt")]
    expires_at: String,
}

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandDownloadResults {
    #[clap(short, long, env)]
    pub run_id: String,

    #[clap(short, long, env, default_value = ".")]
    pub output_dir: PathBuf,

    #[clap(long, env, default_value = "3600")]
    pub expires_in_seconds: u64,
}

#[async_trait]
impl Command for CommandDownloadResults {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            output_dir,
            expires_in_seconds,
        } = self;

        // Generate a random nonce
        let nonce: u64 = rand::random();

        // Create the message to sign: ${runId}:${expiresInSeconds}:${nonce}
        let message = format!("{}:{}:{}", run_id, expires_in_seconds, nonce);
        let message_bytes = message.as_bytes();

        // Sign the message using the backend's wallet
        let signature = backend.sign_message(message_bytes);

        // Encode the signature in base58
        let signature_b58 = bs58::encode(&signature).into_string();

        // Create the request body
        #[derive(Serialize)]
        struct RequestBody {
            #[serde(rename = "expiresInSeconds")]
            expires_in_seconds: u64,
            nonce: String,
        }

        let request_body = RequestBody {
            expires_in_seconds,
            nonce: nonce.to_string(),
        };

        // Make POST request to the API
        let api_url = format!("https://run-down.nousresearch.com/v1/download/{}", run_id);

        let client = reqwest::Client::new();
        let response = client
            .post(&api_url)
            .header("X-Solana-Signature", signature_b58)
            .json(&request_body)
            .send()
            .await
            .context("Failed to fetch download URLs")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("API request failed with status {}: {}", status, error_text);
        }

        let urls_response: DownloadUrlsResponse = response
            .json()
            .await
            .context("Failed to parse response JSON")?;

        println!("Found {} files to download", urls_response.urls.len());

        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(&output_dir)
            .await
            .context("Failed to create output directory")?;

        // Download each file
        for (idx, entry) in urls_response.urls.iter().enumerate() {
            println!(
                "Downloading file {}/{}: {}",
                idx + 1,
                urls_response.urls.len(),
                entry.path
            );

            let file_response = client
                .get(&entry.url)
                .send()
                .await
                .with_context(|| format!("Failed to download file from {}", entry.url))?;

            if !file_response.status().is_success() {
                bail!(
                    "Failed to download file from {}: status {}",
                    entry.url,
                    file_response.status()
                );
            }

            // Preserve the directory structure from the path
            let file_path = output_dir.join(&entry.path);

            // Create parent directories if they don't exist
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| format!("Failed to create directory {:?}", parent))?;
            }

            let bytes = file_response
                .bytes()
                .await
                .with_context(|| format!("Failed to read bytes from {}", entry.url))?;

            tokio::fs::write(&file_path, bytes)
                .await
                .with_context(|| format!("Failed to write file to {:?}", file_path))?;

            println!("  Saved to: {:?}", file_path);
        }

        println!(
            "Successfully downloaded {} files to {:?}",
            urls_response.urls.len(),
            output_dir
        );

        Ok(())
    }
}
