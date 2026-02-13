use std::path::PathBuf;

use crate::commands::Command;
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Args;
use futures::TryStreamExt;
use psyche_solana_rpc::SolanaBackend;
use tokio::fs::File;
use tokio_util::io::StreamReader;

use super::run_down_service;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandDownloadResults {
    #[clap(short, long, env)]
    pub run_id: String,

    #[clap(short, long, env, default_value = ".")]
    pub output_dir: PathBuf,

    #[clap(long, env, default_value = "3600")]
    pub expires_in_seconds: u64,

    #[clap(long)]
    pub overwrite: bool,
}

#[async_trait]
impl Command for CommandDownloadResults {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            output_dir,
            expires_in_seconds,
            overwrite,
        } = self;

        // Check if output directory exists and is not empty
        if output_dir.exists() && !overwrite {
            let mut entries = tokio::fs::read_dir(&output_dir)
                .await
                .context("Failed to read output directory")?;

            if entries.next_entry().await?.is_some() {
                println!(
                    "Warning: Output directory {:?} already exists and contains files.",
                    output_dir
                );
                println!("Files may be overwritten during download.");
                print!("Continue? [y/N]: ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut response = String::new();
                std::io::stdin().read_line(&mut response)?;

                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Download cancelled.");
                    return Ok(());
                }
            }
        }

        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(&output_dir)
            .await
            .context("Failed to create output directory")?;

        // Generate a random nonce
        let nonce: u64 = rand::random();

        // Generate signature for the run-down service
        let signature_b58 =
            run_down_service::generate_signature(&backend, &run_id, expires_in_seconds, nonce);

        // Make POST request to the API
        let client = reqwest::Client::new();
        let urls_response = run_down_service::get_download_urls(
            &client,
            &run_id,
            &signature_b58,
            expires_in_seconds,
            nonce,
        )
        .await?;

        println!("Found {} files to download", urls_response.urls.len());

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

            let mut file = File::create(&file_path)
                .await
                .with_context(|| format!("Failed to create file {file_path:?}"))?;
            let stream = file_response.bytes_stream().map_err(std::io::Error::other);

            tokio::io::copy(&mut StreamReader::new(stream), &mut file)
                .await
                .with_context(|| format!("Failed to download to file {file_path:?}"))?;

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
