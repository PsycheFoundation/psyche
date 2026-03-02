use anyhow::{Context, Result, bail};
use psyche_coordinator::model::{HttpLLMTrainingDataLocation, LLMTrainingDataLocation};
use psyche_core::{Shuffle, TokenSize};
use psyche_data_provider::{
    DataProvider, DataServerConfig, DummyDataProvider, LengthKnownDataProvider, LocalDataProvider,
    PreprocessedDataProvider, Split, WeightedDataProvider, WeightedHttpProvidersConfig,
    http::{FileURLs, HttpDataProvider},
};
use std::path::{Path, PathBuf};
use tracing::info;

pub async fn data_provider_from_location(
    location: &LLMTrainingDataLocation,
    max_seq_len: u32,
    seed: Option<u32>,
) -> Result<DataProvider> {
    let shuffle = seed_to_shuffle(seed);

    match location {
        LLMTrainingDataLocation::Http(HttpLLMTrainingDataLocation {
            location,
            token_size_in_bytes,
            shuffle: data_shuffle,
        }) => {
            let file_urls = FileURLs::from_location(location)
                .await
                .context("Failed to gather list of file URLs")?;
            let provider =
                HttpDataProvider::new(file_urls, *token_size_in_bytes, max_seq_len, *data_shuffle)?;
            info!(
                "Loaded HTTP dataset with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Http(provider))
        }
        LLMTrainingDataLocation::Local(path) => {
            let path_str = path.to_string();
            match LocalDataProvider::new_from_directory(
                &path_str,
                TokenSize::TwoBytes,
                max_seq_len as usize,
                shuffle,
            ) {
                Ok(provider) => {
                    info!(
                        "Loaded local dataset with {} samples",
                        provider.num_sequences()
                    );
                    Ok(DataProvider::Local(provider))
                }
                Err(_) => {
                    let provider = PreprocessedDataProvider::new_from_directory(
                        &path_str,
                        max_seq_len as usize,
                        shuffle,
                        Some(Split::Train),
                        None,
                    )?;
                    info!(
                        "Loaded preprocessed dataset with {} samples",
                        provider.num_sequences()
                    );
                    Ok(DataProvider::Preprocessed(provider))
                }
            }
        }
        LLMTrainingDataLocation::Preprocessed(url) => {
            let url_str = url.to_string();
            let dir = if Path::new(&url_str).exists() {
                PathBuf::from(&url_str)
            } else {
                psyche_data_provider::download_dataset_repo_async(
                    url_str.clone(),
                    None,
                    None,
                    std::env::var("HF_TOKEN").ok(),
                    None,
                    false,
                )
                .await
                .context("Downloading dataset repo failed")?
                .first()
                .ok_or(anyhow::anyhow!("No files downloaded for {url_str}"))?
                .parent()
                .unwrap()
                .into()
            };
            let provider = PreprocessedDataProvider::new_from_directory(
                dir,
                max_seq_len as usize,
                shuffle,
                Some(Split::Train),
                None,
            )?;
            info!(
                "Loaded preprocessed dataset with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Preprocessed(provider))
        }
        LLMTrainingDataLocation::Dummy => Ok(DataProvider::Dummy(DummyDataProvider::new(
            TokenSize::TwoBytes,
            max_seq_len as usize,
            u64::MAX,
        ))),
        LLMTrainingDataLocation::Server(_) => {
            bail!(
                "Server data location not supported for local training — use Local, Http, Preprocessed, or Dummy in your config, or pass --data <data.toml> to load that same data locally"
            )
        }
        LLMTrainingDataLocation::WeightedHttp(json_url) => {
            let config = json_url.to_string();
            let weighted_provider =
                if config.starts_with("http://") || config.starts_with("https://") {
                    WeightedDataProvider::from_config_url(&config, max_seq_len).await?
                } else {
                    // Load from local file
                    let config_content = std::fs::read_to_string(&config)
                        .with_context(|| format!("Failed to read config file: {}", config))?;
                    let config: WeightedHttpProvidersConfig = serde_json::from_str(&config_content)
                        .with_context(|| format!("Failed to parse config JSON: {}", config))?;
                    WeightedDataProvider::from_config(config, max_seq_len).await?
                };
            Ok(DataProvider::WeightedHttp(weighted_provider))
        }
    }
}

pub fn data_provider_from_data_config(data_config: &DataServerConfig) -> Result<DataProvider> {
    let shuffle = Shuffle::Seeded(data_config.shuffle_seed);
    match LocalDataProvider::new_from_directory(
        &data_config.dir,
        data_config.token_size,
        data_config.seq_len,
        shuffle,
    ) {
        Ok(provider) => {
            info!(
                "Loaded local dataset from data.toml with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Local(provider))
        }
        Err(_) => {
            let provider = PreprocessedDataProvider::new_from_directory(
                &data_config.dir,
                data_config.seq_len,
                shuffle,
                Some(Split::Train),
                None,
            )
            .with_context(|| format!("Failed to load data from directory {:?}", data_config.dir))?;
            info!(
                "Loaded preprocessed dataset from data.toml with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Preprocessed(provider))
        }
    }
}

pub fn local_data_provider(
    data_path: &str,
    token_size: usize,
    sequence_length: usize,
    seed: Option<u32>,
) -> Result<DataProvider> {
    let shuffle = seed_to_shuffle(seed);

    match LocalDataProvider::new_from_directory(
        data_path,
        token_size.try_into()?,
        sequence_length,
        shuffle,
    ) {
        Ok(provider) => {
            info!(
                "Loaded local dataset with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Local(provider))
        }
        Err(err) => {
            info!("Failed to load with local data provider: {err:?}. Trying preprocessed instead.");
            let provider = PreprocessedDataProvider::new_from_directory(
                data_path,
                sequence_length,
                shuffle,
                Some(Split::Train),
                None,
            )
            .with_context(|| "Failed to load preprocessed data")?;
            info!(
                "Loaded preprocessed dataset with {} samples",
                provider.num_sequences()
            );
            Ok(DataProvider::Preprocessed(provider))
        }
    }
}

fn seed_to_shuffle(seed: Option<u32>) -> Shuffle {
    match seed {
        Some(x) => {
            let mut array = [0u8; 32];
            array[28..32].copy_from_slice(&x.to_be_bytes());
            Shuffle::Seeded(array)
        }
        None => Shuffle::DontShuffle,
    }
}
