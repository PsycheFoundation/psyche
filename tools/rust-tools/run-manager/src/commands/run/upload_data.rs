use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Args;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::commands::Command;
use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadUrlResponse {
    url: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
}

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandUploadData {
    /// The run ID to upload data to
    #[clap(short, long, env)]
    pub run_id: String,

    /// Path to a single file or directory to upload
    #[clap(short, long)]
    pub path: PathBuf,

    /// How long the signed URLs should be valid (in seconds)
    #[clap(long, env, default_value = "3600")]
    pub expires_in_seconds: u64,
}

#[async_trait]
impl Command for CommandUploadData {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            path,
            expires_in_seconds,
        } = self;

        // Determine base directory for computing relative paths
        let base_dir = if path.is_file() {
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        } else if path.is_dir() {
            path.clone()
        } else {
            bail!(
                "Path does not exist or is not a file or directory: {:?}",
                path
            );
        };

        // Collect all files to upload
        let files_to_upload = if path.is_file() {
            vec![path]
        } else {
            collect_files_from_dir(&path).await?
        };

        if files_to_upload.is_empty() {
            println!("No files found to upload");
            return Ok(());
        }

        println!("Found {} file(s) to upload", files_to_upload.len());

        let client = reqwest::Client::new();

        // Upload each file
        for (idx, file_path) in files_to_upload.iter().enumerate() {
            // Compute relative path from base directory
            let relative_path = file_path
                .strip_prefix(&base_dir)
                .unwrap_or(file_path)
                .to_str()
                .context("Failed to convert path to string")?;

            println!(
                "\n[{}/{}] Uploading: {}",
                idx + 1,
                files_to_upload.len(),
                relative_path
            );

            // Generate a random nonce for this file
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
                filename: String,
                #[serde(rename = "expiresInSeconds")]
                expires_in_seconds: u64,
                nonce: String,
            }

            let request_body = RequestBody {
                filename: relative_path.to_string(),
                expires_in_seconds,
                nonce: nonce.to_string(),
            };

            // Make POST request to get upload URL
            let api_url = format!("https://run-down.nousresearch.com/v1/upload/{}", run_id);

            let response = client
                .post(&api_url)
                .header("X-Solana-Signature", signature_b58)
                .json(&request_body)
                .send()
                .await
                .context("Failed to request upload URL")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                bail!("API request failed with status {}: {}", status, error_text);
            }

            let upload_response: UploadUrlResponse = response
                .json()
                .await
                .context("Failed to parse upload URL response")?;

            // Read the file contents
            let file_contents = fs::read(file_path)
                .await
                .with_context(|| format!("Failed to read file: {:?}", file_path))?;

            let file_size = file_contents.len();
            println!("  Uploading {} bytes...", file_size);

            // Upload the file to the signed URL
            let upload_response = client
                .put(&upload_response.url)
                .header("Content-Type", "application/octet-stream")
                .body(file_contents)
                .send()
                .await
                .with_context(|| format!("Failed to upload file to signed URL"))?;

            if !upload_response.status().is_success() {
                let status = upload_response.status();
                let error_text = upload_response.text().await.unwrap_or_default();
                bail!("Upload failed with status {}: {}", status, error_text);
            }

            println!("  ✓ Upload successful");
        }

        println!(
            "\n✓ Successfully uploaded {} file(s)",
            files_to_upload.len()
        );

        Ok(())
    }
}

/// Recursively collect all files from a directory
async fn collect_files_from_dir(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut entries = fs::read_dir(dir)
        .await
        .with_context(|| format!("Failed to read directory: {:?}", dir))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            let mut sub_files = Box::pin(collect_files_from_dir(&path)).await?;
            files.append(&mut sub_files);
        }
    }

    Ok(files)
}
