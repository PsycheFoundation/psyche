use std::time::Duration;

use anyhow::{Context, Result, bail};
use psyche_core::{BatchId, Shuffle};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use serde::Deserialize;
use tracing::{debug, info};

use crate::{
    TokenizedData,
    traits::{LengthKnownDataProvider, TokenizedDataProvider},
};

const HF_DATASETS_SERVER_BASE_URL: &str = "https://datasets-server.huggingface.co";
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_millis(10000);
const MAX_ROWS_PER_REQUEST: usize = 100;

#[derive(Deserialize, Debug)]
struct InfoResponse {
    dataset_info: DatasetInfo,
}

#[derive(Deserialize, Debug)]
struct DatasetInfo {
    splits: std::collections::HashMap<String, SplitInfo>,
}

#[derive(Deserialize, Debug)]
struct SplitInfo {
    num_examples: usize,
}

#[derive(Deserialize, Debug)]
struct RowsResponse {
    rows: Vec<RowData>,
    #[allow(dead_code)]
    num_rows_total: usize,
}

#[derive(Deserialize, Debug)]
struct RowData {
    row: serde_json::Value,
    #[allow(dead_code)]
    row_idx: usize,
}

pub struct HuggingFacePreprocessedDataProvider {
    client: reqwest::Client,
    dataset: String,
    config: String,
    split: String,
    token: Option<String>,
    num_rows: usize,
    num_tokens_per_sequence: usize,
    shuffle_indices: Option<Vec<usize>>,
}

impl HuggingFacePreprocessedDataProvider {
    pub async fn new(
        dataset: String,
        config: String,
        split: String,
        token: Option<String>,
        num_tokens_per_sequence: usize,
        shuffle: Shuffle,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_REQUEST_TIMEOUT)
            .build()
            .context("Failed to create HTTP client")?;

        // Fetch dataset metadata to get row count
        let num_rows = Self::fetch_num_rows(&client, &dataset, &config, &split, &token).await?;
        info!(
            "Initialized HuggingFace preprocessed data provider for {}/{}/{} with {} rows",
            dataset, config, split, num_rows
        );

        // Build shuffle indices if needed
        let shuffle_indices = match shuffle {
            Shuffle::Seeded(seed) => {
                let mut indices: Vec<usize> = (0..num_rows).collect();
                indices.shuffle(&mut ChaCha8Rng::from_seed(seed));
                Some(indices)
            }
            Shuffle::DontShuffle => None,
        };

        Ok(Self {
            client,
            dataset,
            config,
            split,
            token,
            num_rows,
            num_tokens_per_sequence,
            shuffle_indices,
        })
    }

    async fn fetch_num_rows(
        client: &reqwest::Client,
        dataset: &str,
        config: &str,
        split: &str,
        token: &Option<String>,
    ) -> Result<usize> {
        let url = format!(
            "{}/info?dataset={}&config={}",
            HF_DATASETS_SERVER_BASE_URL, dataset, config
        );

        let response = make_authenticated_request(client, &url, token)
            .await
            .context("Failed to fetch dataset info from HuggingFace")?;

        let info: InfoResponse = response
            .json()
            .await
            .context("Failed to parse dataset info response")?;

        let split_info = info
            .dataset_info
            .splits
            .get(split)
            .ok_or_else(|| anyhow::anyhow!("Split '{}' not found in dataset", split))?;

        Ok(split_info.num_examples)
    }

    async fn fetch_rows(&self, offset: usize, length: usize) -> Result<Vec<TokenizedData>> {
        let url = format!(
            "{}/rows?dataset={}&config={}&split={}&offset={}&length={}",
            HF_DATASETS_SERVER_BASE_URL, self.dataset, self.config, self.split, offset, length
        );

        debug!(
            "Fetching rows from HuggingFace: offset={}, length={}",
            offset, length
        );

        let response = make_authenticated_request(&self.client, &url, &self.token)
            .await
            .context("Failed to fetch rows from HuggingFace")?;

        let rows_response: RowsResponse = response
            .json()
            .await
            .context("Failed to parse rows response")?;

        // Convert JSON rows to TokenizedData
        let mut tokenized_data = Vec::with_capacity(rows_response.rows.len());
        for row_data in rows_response.rows {
            let tokenized = self.parse_row(row_data.row)?;
            tokenized_data.push(tokenized);
        }

        Ok(tokenized_data)
    }

    fn parse_row(&self, row: serde_json::Value) -> Result<TokenizedData> {
        let obj = row
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Row is not a JSON object"))?;

        // Depending on the repo sometimes 'inputs' or 'input_ids' is used
        let input_ids = if let Some(inputs) = obj.get("inputs") {
            parse_int_array(inputs, "inputs", Some(self.num_tokens_per_sequence))?
        } else if let Some(input_ids) = obj.get("input_ids") {
            parse_int_array(input_ids, "input_ids", Some(self.num_tokens_per_sequence))?
        } else {
            bail!("Missing 'inputs' or 'input_ids' column");
        };

        // These columns are optional and they might or might not be present
        let labels = obj
            .get("labels")
            .map(|v| parse_int_array(v, "labels", Some(self.num_tokens_per_sequence)))
            .transpose()?;

        let position_ids = obj
            .get("position_ids")
            .map(|v| parse_int_array(v, "position_ids", Some(self.num_tokens_per_sequence)))
            .transpose()?;

        let sequence_lengths = obj
            .get("sequence_lengths")
            .map(|v| parse_int_array(v, "sequence_lengths", None))
            .transpose()?;

        Ok(TokenizedData {
            input_ids,
            labels,
            position_ids,
            sequence_lengths,
        })
    }

    fn map_batch_id_to_rows(&self, batch_id: BatchId) -> Vec<usize> {
        if self.num_rows == 0 {
            return vec![];
        }

        let (start, end) = (batch_id.0.start as usize, batch_id.0.end as usize);
        (start..=end)
            .map(|i| {
                let idx = i % self.num_rows;
                self.shuffle_indices.as_ref().map_or(idx, |s| s[idx])
            })
            .collect()
    }
}

impl TokenizedDataProvider for HuggingFacePreprocessedDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        if self.num_rows == 0 {
            bail!("No data available");
        }

        // Batch consecutive indices together for efficient API calls
        let row_indices = self.map_batch_id_to_rows(data_ids);
        let mut samples = Vec::with_capacity(row_indices.len());

        // For example if row_indices = [5, 6, 7, 10, 11] then the loop does:
        // - [5, 6, 7] in first iteration since they are consecutive
        // - [10, 11] in second iteration
        let mut i = 0;
        while i < row_indices.len() {
            let start_idx = row_indices[i];
            let mut length = 1;

            // Find consecutive indices (e.g. 5, 6, 7) (up to MAX_ROWS_PER_REQUEST)
            while i + length < row_indices.len()
                && length < MAX_ROWS_PER_REQUEST
                && row_indices[i + length] == start_idx + length
            {
                length += 1;
            }

            // Fetch this batch
            let batch_samples = self.fetch_rows(start_idx, length).await?;
            samples.extend(batch_samples);

            i += length;
        }

        if samples.len() != row_indices.len() {
            bail!(
                "Expected {} rows but got {}",
                row_indices.len(),
                samples.len()
            );
        }

        Ok(samples)
    }
}

impl LengthKnownDataProvider for HuggingFacePreprocessedDataProvider {
    fn num_sequences(&self) -> usize {
        self.num_rows
    }
}

async fn make_authenticated_request(
    client: &reqwest::Client,
    url: &str,
    token: &Option<String>,
) -> Result<reqwest::Response> {
    let mut request = client.get(url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("HTTP {} - {}. URL: {}", status, body, url);
    }

    Ok(response)
}

fn parse_int_array(
    value: &serde_json::Value,
    column_name: &str,
    required_len: Option<usize>,
) -> Result<Vec<i32>> {
    let array = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Column '{}' is not an array", column_name))?;

    let ret = array
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| anyhow::anyhow!("Non-integer value in column '{}'", column_name))
                .map(|i| i as i32)
        })
        .collect::<Result<Vec<i32>, _>>()?;

    if let Some(required_len) = required_len {
        let len = ret.len();
        if len != required_len {
            bail!("`{column_name}` has length {len} instead of {required_len}");
        }
    }

    Ok(ret)
}
