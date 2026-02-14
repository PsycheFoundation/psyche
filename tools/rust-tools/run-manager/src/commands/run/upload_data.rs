use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Args;
use tokio::fs::File;
use walkdir::WalkDir;

use crate::commands::Command;
use psyche_solana_rpc::SolanaBackend;

use super::run_down_service;

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

            // Generate signature for the run-down service
            let signature_b58 =
                run_down_service::generate_signature(&backend, &run_id, expires_in_seconds, nonce);

            // Make POST request to get upload URL
            let upload_response = run_down_service::get_upload_url(
                &client,
                &run_id,
                &signature_b58,
                relative_path,
                expires_in_seconds,
                nonce,
            )
            .await?;

            // Stream the file contents
            let file = File::open(file_path)
                .await
                .with_context(|| format!("Failed to read file: {:?}", file_path))?;

            let file_metadata = file
                .metadata()
                .await
                .with_context(|| format!("Failed to read file metadata: {:?}", file))?;
            let file_size = file_metadata.len();
            println!("  Uploading {} bytes...", file_size);

            // Upload the file to the signed URL
            let upload_response = client
                .put(&upload_response.url)
                .header("Content-Type", "application/octet-stream")
                .body(file)
                .send()
                .await
                .context("Failed to upload file to signed URL")?;

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
    let files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect();

    Ok(files)
}
