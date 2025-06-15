use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::future::join_all;
use psyche_core::{BatchId, Shuffle, TokenSize};
use psyche_data_provider::{
    http::{FileURLs, HttpDataProvider},
    TokenizedDataProvider, WeightedDataProvider,
};
use tokio::time::Instant;
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
    /* Template {
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
    },*/
    /// A Config URL
    ConfigUrl {
        /// List of data URLs, in order (e.g., "http://example.com/1.ds", "http://example.com/2.ds")
        url: String,
    },
    /// A fixed list of URLs
    Urls {
        /// List of data URLs, in order (e.g., "http://example.com/1.ds", "http://example.com/2.ds")
        urls: Vec<String>,
    },
    // A public GCP bucket
    /*Gcp {
        /// The name of the GCP bucket
        bucket_name: String,
        /// An optional directory to filter by
        directory: Option<String>,
    },*/
}

#[tokio::main]
async fn main() -> Result<()> {
    let start = Instant::now();

    let cli = Cli::parse();

    let token_size: TokenSize = cli.token_size.try_into()?;

    let batch_ids: Vec<BatchId> = cli
        .batch_ids
        .into_iter()
        .map(|x| BatchId((x, x).into()))
        .collect();
    if batch_ids.is_empty() {
        anyhow::bail!("At least one batch ID must be specified");
    }

    let mut provider = match cli.command {
        /*Commands::Template {
            template,
            start,
            left_pad_zeros,
            end,
        } => FileURLs::from_template(&template, start, left_pad_zeros, end - start).await?,*/
        Commands::ConfigUrl { url } => {
            if url.is_empty() {
                anyhow::bail!("at least one URL must be passed");
            }
            WeightedDataProvider::from_config_url(&url, cli.sequence_length)
                .await
                .unwrap()
        }
        Commands::Urls { urls } => {
            if urls.is_empty() {
                anyhow::bail!("at least one URL must be passed");
            }

            // todo: is this right? should we we creating a new HttpDataProvider for each url???
            let providers = join_all(urls.into_iter().map(|url| async {
                let file_urls = FileURLs::from_list(&vec![url]).await.unwrap();
                HttpDataProvider::new(file_urls, token_size, cli.sequence_length, Shuffle::DontShuffle).unwrap()
            })).await;

            WeightedDataProvider::new(providers, Shuffle::DontShuffle)
        } /*Commands::Gcp {
              bucket_name,
              directory,
          } => FileURLs::from_gcp_bucket(&bucket_name, directory).await?,*/
    };
    /*let provider =
    HttpDataProvider::new(urls, token_size, cli.sequence_length, Shuffle::DontShuffle)?;*/

    let tokenizer = cli.tokenizer.map(|tokenizer_path: PathBuf| {
        Tokenizer::from_file(tokenizer_path).expect("tokenizer exists")
    });
    for batch in batch_ids {
        let samples = provider.get_samples(batch).await?;

        // Output handling
        if let Some(tokenizer) = &tokenizer {
            for (i, sample) in samples.iter().enumerate() {
                println!("=== Batch {} Sample {} ===", batch.0.start, i);
                let decoded = tokenizer
                    .decode(&sample.iter().map(|&x| x as u32).collect::<Vec<_>>(), false)
                    .expect("tokenizer decode worked");
                println!("{}", decoded);
                println!();
            }
        } else {
            for (i, sample) in samples.iter().enumerate() {
                println!("=== Batch {} Sample {} ===", batch.0.start, i);
                println!("{:?}", sample);
                println!();
            }
        }
    }
    
    let elapsed  = start.elapsed();
    println!("Duration (in secs): {}", elapsed.as_secs_f64());

    Ok(())
}
