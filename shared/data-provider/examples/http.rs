use anyhow::Result;
use clap::{Parser, Subcommand};
use psyche_core::{BatchId, Shuffle, TokenSize};
use psyche_data_provider::{
    http::{FileURLs, HttpDataProvider},
    TokenizedDataProvider,
};
use std::path::PathBuf;
use tokenizers::Tokenizer;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sequence length
    #[arg(long, default_value = "2048")]
    sequence_length: u32,

    /// Token size in bytes
    #[arg(long, default_value = "2")]
    token_size: usize,

    /// Batch IDs to retrieve (comma-separated)
    #[arg(long, use_value_delimiter = true)]
    batch_ids: Vec<u64>,

    /// Optional tokenizer path for decoding output
    #[arg(long)]
    tokenizer: Option<PathBuf>,

    /// Where to pull samples from
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// A URL template
    Template {
        /// URL template with {} placeholder (e.g., "http://example.com/{}.ds")
        template: String,
        /// Start index
        #[arg(long, default_value = "0")]
        start: u32,
        /// End index
        #[arg(long)]
        end: u32,
        // number of zeros to left-pad to
        #[arg(long, default_value = "0")]
        left_pad_zeros: u8,
    },
    /// A fixed list of URLs
    Urls {
        /// List of data URLs, in order (e.g., "http://example.com/1.ds", "http://example.com/2.ds")
        urls: Vec<String>,
    },
    /// A public GCP bucket
    Gcp {
        /// The URL of the GCP bucket
        bucket_url: String,
        /// An optional directory to filter by
        directory: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let token_size: TokenSize = cli.token_size.try_into()?;

    let batch_ids: Vec<BatchId> = cli.batch_ids.into_iter().map(BatchId::from_u64).collect();
    if batch_ids.is_empty() {
        anyhow::bail!("At least one batch ID must be specified");
    }

    let urls = match cli.command {
        Commands::Template {
            template,
            start,
            left_pad_zeros,
            end,
        } => FileURLs::from_template(&template, start, left_pad_zeros, end - start).await?,
        Commands::Urls { urls } => {
            if urls.is_empty() {
                anyhow::bail!("at least one URL must be passed");
            }
            FileURLs::from_list(&urls).await?
        }
        Commands::Gcp {
            bucket_url,
            directory,
        } => FileURLs::from_gcp_bucket(&bucket_url, directory).await?,
    };
    let mut provider =
        HttpDataProvider::new(urls, token_size, cli.sequence_length, Shuffle::DontShuffle)?;
    let samples = provider.get_samples(&batch_ids).await?;

    // Output handling
    if let Some(tokenizer_path) = cli.tokenizer {
        let tokenizer = Tokenizer::from_file(tokenizer_path).expect("tokenizer exists");
        for (i, sample) in samples.iter().enumerate() {
            println!("=== Batch {} ===", batch_ids[i]);
            let decoded = tokenizer
                .decode(&sample.iter().map(|&x| x as u32).collect::<Vec<_>>(), true)
                .expect("tokenizer decode worked");
            println!("{}", decoded);
            println!();
        }
    } else {
        for (i, sample) in samples.iter().enumerate() {
            println!("=== Batch {} ===", batch_ids[i]);
            println!("{:?}", sample);
            println!();
        }
    }

    Ok(())
}
