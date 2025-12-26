use psyche_coordinator::model::HttpLLMTrainingDataLocation;
use psyche_core::Shuffle;

use crate::http::{FileURLs, HttpDataProvider};

use super::{Providers, WeightedDataProvider};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

impl WeightedDataProvider<HttpDataProvider> {
    pub async fn from_config(
        config: WeightedHttpProvidersConfig,
        max_seq_len: u32,
    ) -> Result<Self> {
        let weights = match &config.providers {
            HttpProviderConfigs::ExplicitlyWeighted(items) => {
                Some(items.iter().map(|(_, w)| *w).collect::<Vec<_>>())
            }
            HttpProviderConfigs::LengthWeighted(_) => None,
        };
        let http_provider_configs = match config.providers {
            HttpProviderConfigs::ExplicitlyWeighted(items) => {
                items.into_iter().map(|(p, _)| p).collect()
            }
            HttpProviderConfigs::LengthWeighted(items) => items,
        };
        let mut http_providers = vec![];
        for HttpLLMTrainingDataLocation {
            location,
            token_size_in_bytes,
            shuffle,
        } in http_provider_configs
        {
            let file_urls = FileURLs::from_location(&location).await?;
            let provider =
                HttpDataProvider::new(file_urls, token_size_in_bytes, max_seq_len, shuffle)?;
            http_providers.push(provider)
        }
        let providers: Providers<HttpDataProvider> = match weights {
            Some(weights) => weights
                .into_iter()
                .zip(http_providers)
                .map(|(weight, provider)| (provider, weight))
                .collect::<Vec<_>>()
                .into(),
            None => http_providers.into(),
        };
        Ok(WeightedDataProvider::new(providers, config.shuffle))
    }

    pub async fn from_config_url(url: &str, max_seq_len: u32) -> Result<Self> {
        let client = reqwest::Client::new();
        let config: WeightedHttpProvidersConfig = client.get(url).send().await?.json().await?;
        Self::from_config(config, max_seq_len).await
    }
}

#[derive(Serialize, Deserialize, TS, Debug)]
#[ts(export)]
pub struct WeightedHttpProvidersConfig {
    shuffle: Shuffle,
    providers: HttpProviderConfigs,
}

#[derive(Serialize, Deserialize, TS, Debug)]
#[serde(untagged)]
pub enum HttpProviderConfigs {
    /// Weights will be normalized to their sum. e.g. weights 1.0, 1.0, 2.0 will normalize to 0.25, 0.25, 0.5
    ExplicitlyWeighted(Vec<(HttpLLMTrainingDataLocation, f64)>),
    /// Weights will be derived from dataset lengths, and normalized.
    LengthWeighted(Vec<HttpLLMTrainingDataLocation>),
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use psyche_coordinator::model::{HttpLLMTrainingDataLocation, HttpTrainingDataLocation};
    use psyche_core::{BatchId, Shuffle, TokenSize};
    use std::{
        fs::{self, File},
        io::Write,
        net::SocketAddr,
        time::Duration,
    };
    use tempfile::TempDir;
    use test_log::test;
    use tokio::time::timeout;
    use tracing::{debug, info};

    use crate::{TokenizedDataProvider, WeightedDataProvider, http::HttpDataProvider};

    use super::WeightedHttpProvidersConfig;

    struct TestServer {
        cancel: tokio::sync::watch::Sender<()>,
        addr: SocketAddr,
        // just so it doesn't get dropped
        _temp_dir: TempDir,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.cancel.send(()).unwrap();
        }
    }

    impl TestServer {
        async fn new(data_providers: Vec<Vec<Vec<u8>>>) -> Result<Self> {
            let temp_dir = tempfile::tempdir()?;

            for (provider_idx, files) in data_providers.iter().enumerate() {
                let provider_folder = temp_dir.path().join(format!("provider_{provider_idx}"));
                fs::create_dir_all(&provider_folder)?;
                for (idx, data) in files.iter().enumerate() {
                    let file_path = provider_folder.clone().join(format!("{idx:0>3}.ds"));
                    let mut file = File::create(&file_path)?;
                    file.write_all(data)?;
                    debug!("created temp test file {file_path:?}");
                }
            }

            let (cancel, rx_cancel) = tokio::sync::watch::channel(());
            let mut settings = static_web_server::Settings::get_unparsed(false)?;
            settings.general.port = 0;
            settings.general.root = temp_dir.path().to_path_buf();
            settings.general.directory_listing = true;

            let (tx_port, rx_port) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                static_web_server::Server::new(settings)
                    .unwrap()
                    .run_standalone(Some(rx_cancel), tx_port)
                    .unwrap();
            });
            let port = rx_port.await?;
            let addr = SocketAddr::new("127.0.0.1".parse()?, port);

            let multi_config = WeightedHttpProvidersConfig {
                shuffle: Shuffle::DontShuffle,
                providers: super::HttpProviderConfigs::ExplicitlyWeighted(
                    data_providers
                        .iter()
                        .enumerate()
                        .map(|(provider_idx, files)| {
                            let url_template =
                                format!("http://{}/provider_{provider_idx}/{{}}.ds", &addr);
                            (
                                HttpLLMTrainingDataLocation {
                                    location: HttpTrainingDataLocation::NumberedFiles {
                                        url_template: url_template.as_str().try_into().unwrap(),
                                        start_index: 0,
                                        n_left_pad_zeros: 3,
                                        num_files: files.len() as u32,
                                    },
                                    token_size_in_bytes: TokenSize::TwoBytes,
                                    shuffle: Shuffle::DontShuffle,
                                },
                                1.0,
                            )
                        })
                        .collect(),
                ),
            };

            let multi_config_json = serde_json::to_string(&multi_config).unwrap();
            let file_path = temp_dir.path().join("multi_config.json");
            let mut multi_config_file = File::create(&file_path)?;
            multi_config_file.write_all(multi_config_json.as_bytes())?;
            debug!("wrote multi config {file_path:?}");
            info!("server running at {addr}");
            Ok(Self {
                addr,
                cancel,
                _temp_dir: temp_dir,
            })
        }
    }

    #[test(tokio::test)]
    async fn test_http_multi_data_provider() -> Result<()> {
        const FILE_SIZE: u64 = 16;
        const SEQUENCE_LEN: u32 = 3;

        let file1: Vec<u8> = (0..FILE_SIZE).map(|_| 1_u8).collect();
        let file2: Vec<u8> = (FILE_SIZE..FILE_SIZE * 2).map(|_| 2_u8).collect();

        let file3: Vec<u8> = (0..FILE_SIZE).map(|_| 3_u8).collect();
        let file4: Vec<u8> = (FILE_SIZE..FILE_SIZE * 2).map(|_| 4_u8).collect();

        let server = TestServer::new(vec![
            vec![file1.clone(), file2.clone()],
            vec![file3.clone(), file4.clone()],
        ])
        .await?;

        let multi_config_addr = format!("http://{}/multi_config.json", server.addr);
        println!("fetching multi config from {multi_config_addr}");

        let mut provider = WeightedDataProvider::<HttpDataProvider>::from_config_url(
            &multi_config_addr,
            SEQUENCE_LEN,
        )
        .await?;

        // Test first sequence
        println!("first sequence..");
        let samples = timeout(
            Duration::from_secs(2),
            provider.get_samples(BatchId((0, 8).into())),
        )
        .await??
        .into_iter()
        .map(|x| x.input_ids)
        .collect::<Vec<_>>();

        assert_eq!(samples.len(), 9);
        assert_eq!(
            &samples,
            &[
                [
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                ],
                [
                    i32::from_le_bytes([3, 3, 0, 0]),
                    i32::from_le_bytes([3, 3, 0, 0]),
                    i32::from_le_bytes([3, 3, 0, 0]),
                ],
                [
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                ],
                [
                    i32::from_le_bytes([3, 3, 0, 0]),
                    i32::from_le_bytes([3, 3, 0, 0]),
                    i32::from_le_bytes([3, 3, 0, 0]),
                ],
                [
                    i32::from_le_bytes([2, 2, 0, 0]),
                    i32::from_le_bytes([2, 2, 0, 0]),
                    i32::from_le_bytes([2, 2, 0, 0]),
                ],
                [
                    i32::from_le_bytes([4, 4, 0, 0]),
                    i32::from_le_bytes([4, 4, 0, 0]),
                    i32::from_le_bytes([4, 4, 0, 0]),
                ],
                [
                    i32::from_le_bytes([2, 2, 0, 0]),
                    i32::from_le_bytes([2, 2, 0, 0]),
                    i32::from_le_bytes([2, 2, 0, 0]),
                ],
                [
                    i32::from_le_bytes([4, 4, 0, 0]),
                    i32::from_le_bytes([4, 4, 0, 0]),
                    i32::from_le_bytes([4, 4, 0, 0]),
                ],
                // at this point we run out of samples and we just.. serve whatever
                [
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                    i32::from_le_bytes([1, 1, 0, 0]),
                ],
            ]
        );

        Ok(())
    }
}
