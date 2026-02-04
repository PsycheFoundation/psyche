use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use futures::future::join_all;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use psyche_coordinator::model::HttpTrainingDataLocation;
use psyche_core::{BatchId, Shuffle, TokenSize};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use reqwest::IntoUrl;
use tokio::task::JoinHandle;
use tracing::{info, trace};

use crate::{
    TokenizedData,
    file_extensions::DATA_FILE_EXTENSIONS,
    traits::{LengthKnownDataProvider, TokenizedDataProvider},
};

const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_millis(5000);

#[derive(Clone, Copy, Debug)]
struct SequencePointer {
    file_index: usize,
    byte_offset: usize,
}

pub struct HttpDataProvider {
    client: reqwest::Client,
    file_urls: Vec<reqwest::Url>,
    sequences: Vec<SequencePointer>,
    seq_len: u32,
    token_size_in_bytes: TokenSize,
}

impl LengthKnownDataProvider for HttpDataProvider {
    fn num_sequences(&self) -> usize {
        self.sequences.len()
    }
}

/// A Vec of (url, file size)
pub struct FileURLs(Vec<(reqwest::Url, u64)>);

impl FileURLs {
    pub async fn from_list(urls: &[impl IntoUrl + Clone]) -> Result<Self, anyhow::Error> {
        let client = reqwest::Client::new();
        let urls: Result<Vec<reqwest::Url>, reqwest::Error> =
            urls.iter().map(|u| u.clone().into_url()).collect();
        let urls_with_sizes = with_file_sizes(&client, &urls?).await?;

        Ok(FileURLs(urls_with_sizes))
    }

    pub async fn from_template(
        url_template: &str,
        start_index: u32,
        n_left_pad_zeros: u8,
        num_files: u32,
    ) -> Result<Self> {
        let num_templates = url_template
            .as_bytes()
            .windows(2)
            .filter(|&w| w == "{}".as_bytes())
            .count();
        if num_templates != 1 {
            bail!(
                "invalid url {url_template}. expected 1 set of {{}} for number substitution, got {num_templates}"
            );
        }

        let urls: Result<Vec<reqwest::Url>, <reqwest::Url as FromStr>::Err> = (0..num_files)
            .map(|index| {
                let number = start_index + index;
                let formatted_number =
                    format!("{:0>width$}", number, width = n_left_pad_zeros as usize);
                url_template.replace("{}", &formatted_number).parse()
            })
            .collect();

        let client = reqwest::Client::new();
        let urls_with_sizes = with_file_sizes(&client, &urls?).await?;

        Ok(Self(urls_with_sizes))
    }

    pub async fn from_gcp_bucket(bucket_name: &str, directory: Option<String>) -> Result<Self> {
        let config = google_cloud_storage::client::ClientConfig::default().anonymous();
        let client = google_cloud_storage::client::Client::new(config);
        let mut data_files_matching_directory = {
            let mut all_results = vec![];
            // the outer option is if we should continue looping
            // the inner option is if we have a "next page token"
            let mut next_page_token: Option<Option<String>> = Some(None);

            while let Some(maybe_next_page_token) = next_page_token {
                let this_results = client
                    .list_objects(&ListObjectsRequest {
                        bucket: bucket_name.to_owned(),
                        prefix: directory.clone(),
                        page_token: maybe_next_page_token,
                        ..Default::default()
                    })
                    .await?;
                all_results.extend(this_results.items.iter().flatten().filter_map(|obj| {
                    let file_ext = obj.name.split('.').next_back()?;
                    if !DATA_FILE_EXTENSIONS.contains(&file_ext) {
                        return None;
                    }

                    Some(
                        obj.media_link
                            .parse::<reqwest::Url>()
                            .map(|full_url| (full_url, obj.size as u64))
                            .map_err(anyhow::Error::from),
                    )
                }));

                // if we have a token, Some(Some(String)),
                // if not, None
                next_page_token = this_results.next_page_token.map(Some)
            }
            all_results
        }
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

        data_files_matching_directory.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(Self(data_files_matching_directory))
    }

    pub async fn from_location(location: &HttpTrainingDataLocation) -> Result<Self> {
        match location {
            HttpTrainingDataLocation::NumberedFiles {
                url_template,
                start_index,
                n_left_pad_zeros,
                num_files,
            } => {
                Self::from_template(
                    &String::from(url_template),
                    *start_index,
                    *n_left_pad_zeros,
                    *num_files,
                )
                .await
            }
            HttpTrainingDataLocation::SingleUrl(url) => Self::from_list(&[String::from(url)]).await,
            HttpTrainingDataLocation::Gcp {
                bucket_name,
                filter_directory,
            } => {
                let filter_directory = String::from(filter_directory);
                Self::from_gcp_bucket(
                    &String::from(bucket_name),
                    if filter_directory.is_empty() {
                        None
                    } else {
                        Some(filter_directory)
                    },
                )
                .await
            }
        }
    }
}

impl HttpDataProvider {
    pub fn new(
        file_urls: FileURLs,
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: u32,
        shuffle: Shuffle,
    ) -> Result<Self> {
        let file_urls = file_urls.0;
        let num_files = file_urls.len();

        let client = reqwest::Client::new();

        let seq_len_in_bytes =
            num_tokens_per_sequence as u64 * usize::from(token_size_in_bytes) as u64;

        let sequences: Vec<SequencePointer> = {
            let mut all_indexes: Vec<_> = (0..num_files)
                .flat_map(|file_index| {
                    let file_size = file_urls[file_index].1;
                    (0..file_size - (seq_len_in_bytes + usize::from(token_size_in_bytes) as u64)) // +1 token for pretraining data!
                        .step_by(seq_len_in_bytes as usize)
                        .map(move |byte_offset| SequencePointer {
                            file_index,
                            byte_offset: byte_offset as usize,
                        })
                })
                .collect();

            if let Shuffle::Seeded(seed) = shuffle {
                let mut rng = ChaCha8Rng::from_seed(seed);
                all_indexes.shuffle(&mut rng);
            }
            all_indexes
        };

        info!(
            "Created HTTP data provider for {} files with {} sequences",
            num_files,
            sequences.len()
        );

        Ok(Self {
            client,
            file_urls: file_urls.into_iter().map(|f| f.0).collect(),
            sequences,
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        })
    }

    async fn fetch_data_range(
        client: reqwest::Client,
        url: reqwest::Url,
        start: usize,
        length: usize,
    ) -> Result<Vec<u8>> {
        trace!(
            "requesting bytes={}-{} from {url}",
            start,
            start + length - 1
        );

        let range = format!("bytes={}-{}", start, start + length - 1);
        let response = client
            .get(url.clone())
            .header("Range", &range)
            .timeout(HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("request for bytes {range}, from {url}"))?;

        // Check if we got a 206 Partial Content response
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Server returned unexpected status code: {}",
                response.status()
            ));
        }

        let bytes = response.bytes().await?;
        let received_length = bytes.len();

        // Verify we got the expected amount of data
        if received_length != length {
            return Err(anyhow::anyhow!(
                "Received unexpected number of bytes: got {}, expected {}",
                received_length,
                length
            ));
        }

        Ok(bytes.to_vec())
    }

    async fn fetch_tokenized_data_range(
        client: reqwest::Client,
        url: reqwest::Url,
        start: usize,
        length: usize,
        token_size_in_bytes: TokenSize,
    ) -> Result<Vec<i32>> {
        let data = Self::fetch_data_range(client, url, start, length).await?;

        let tokens: Vec<i32> = data
            .chunks(token_size_in_bytes.into())
            .map(|t| {
                use TokenSize::*;
                match token_size_in_bytes {
                    TwoBytes => u16::from_le_bytes(t.try_into().unwrap()) as i32,
                    FourBytes => u32::from_le_bytes(t.try_into().unwrap()) as i32,
                }
            })
            .collect();

        Ok(tokens)
    }

    async fn internal_get_samples(&self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        trace!("get samples for {data_ids:?}");

        let sequences: Result<Vec<SequencePointer>> = data_ids
            .iter()
            .map(|data_id| {
                self.sequences
                    .get(data_id as usize)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow!(
                            "index {data_id} is out of bounds, we only have {} samples.",
                            self.sequences.len()
                        )
                    })
            })
            .collect();
        let sequences = sequences?;

        // check if this is fully sequential (all in the same file and with contiguous offsets)
        let first_file_index = sequences[0].file_index;
        let token_size = usize::from(self.token_size_in_bytes);
        let single_seq_len = self.seq_len as usize;
        let single_seq_bytes = token_size * (self.seq_len as usize); // bytes for seq_len tokens (not including overlap)

        let is_sequential = sequences.iter().all(|x| x.file_index == first_file_index)
            && sequences
                .windows(2)
                .all(|x| x[1].byte_offset - x[0].byte_offset == single_seq_bytes);

        if is_sequential && sequences.len() > 1 {
            // for sequential access, read the entire range at once
            let start_offset = sequences[0].byte_offset;
            let total_length = single_seq_bytes * sequences.len();

            trace!(
                length = total_length,
                offset = start_offset,
                url = %self.file_urls[first_file_index],
                "Sequential data access",
            );

            let all_data = Self::fetch_tokenized_data_range(
                self.client.clone(),
                self.file_urls[first_file_index].clone(),
                start_offset,
                total_length,
                self.token_size_in_bytes,
            )
            .await?;

            // split the data into individual sequences with one token of overlap
            let mut result = Vec::with_capacity(sequences.len());
            for i in 0..sequences.len() {
                let start_idx = i * self.seq_len as usize;
                let end_idx = start_idx + single_seq_len;
                result.push(TokenizedData::from_input_ids(
                    all_data[start_idx..end_idx].to_vec(),
                ));
            }

            Ok(result)
        } else {
            trace!(
                num_sequences = sequences.len(),
                "Non-sequential data access",
            );

            let mut futures = Vec::new();
            let data_len = usize::from(self.token_size_in_bytes) * self.seq_len as usize;
            for sequence in sequences {
                let future: JoinHandle<Result<Vec<i32>>> =
                    tokio::spawn(Self::fetch_tokenized_data_range(
                        self.client.clone(),
                        self.file_urls[sequence.file_index].clone(),
                        sequence.byte_offset,
                        data_len,
                        self.token_size_in_bytes,
                    ));

                futures.push(future);
            }
            let finished = join_all(futures.into_iter()).await;

            let mut ret = Vec::with_capacity(finished.len());
            for finish in finished {
                ret.push(TokenizedData::from_input_ids(finish??));
            }

            Ok(ret)
        }
    }
}

impl TokenizedDataProvider for HttpDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        self.internal_get_samples(data_ids).await
    }
}

// i tried this nicely with streams and generators.
// there's some weird rust impl is not general enough for Send bug i hit
// so i just manually chunk instead of doing it fancy with a limited concurrency stream
async fn with_file_sizes(
    client: &reqwest::Client,
    urls: &[reqwest::Url],
) -> Result<Vec<(reqwest::Url, u64)>> {
    let futures: Vec<_> = urls
        .iter()
        .map(|url| {
            let url = url.clone();
            async move {
                let response = client.head(url.clone()).send().await?;

                if !response.status().is_success() {
                    bail!("URL validation failed for {}: {}", url, response.status());
                }

                // grab the Content-Length header
                let size = response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Missing or invalid Content-Length header for {}", url)
                    })?;
                Ok::<(reqwest::Url, u64), anyhow::Error>((url, size))
            }
        })
        .collect();

    let mut results = Vec::with_capacity(urls.len());
    let mut futures = futures.into_iter();

    // only pull 2 chunks at once
    while let Some(first) = futures.next() {
        let mut chunk = vec![first];
        for _ in 0..2 {
            if let Some(next) = futures.next() {
                chunk.push(next);
            } else {
                break;
            }
        }

        let chunk_results = futures::future::join_all(chunk).await;
        for result in chunk_results {
            results.push(result?);
        }
    }

    Ok(results)
}
