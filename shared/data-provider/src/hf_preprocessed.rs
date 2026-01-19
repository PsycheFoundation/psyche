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
    shuffle_indices: Option<Vec<usize>>,
}

impl HuggingFacePreprocessedDataProvider {
    pub async fn new(
        dataset: String,
        config: String,
        split: String,
        token: Option<String>,
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

        let mut request = client.get(&url);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .context("Failed to fetch dataset info from HuggingFace")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "Failed to fetch dataset info: {} - {}. URL: {}",
                status,
                body,
                url
            );
        }

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

        let mut request = self.client.get(&url);
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        debug!(
            "Fetching rows from HuggingFace: offset={}, length={}",
            offset, length
        );

        let response = request
            .send()
            .await
            .context("Failed to fetch rows from HuggingFace")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to fetch rows: {} - {}. URL: {}", status, body, url);
        }

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
            self.parse_int_array(inputs, None, "inputs")?
        } else if let Some(input_ids) = obj.get("input_ids") {
            self.parse_int_array(input_ids, None, "input_ids")?
        } else {
            bail!("Missing 'inputs' or 'input_ids' column");
        };

        // Parse optional columns - also handle length mismatch
        let labels = obj
            .get("labels")
            .map(|v| -> Result<Vec<i32>> { self.parse_int_array(v, None, "labels") })
            .transpose()?;

        let position_ids = obj
            .get("position_ids")
            .map(|v| -> Result<Vec<i32>> { self.parse_int_array(v, None, "position_ids") })
            .transpose()?;

        let sequence_lengths = obj
            .get("sequence_lengths")
            .map(|v| self.parse_int_array(v, None, "sequence_lengths"))
            .transpose()?;

        // Debug logging to verify data is being parsed correctly
        let labels_info = labels.as_ref().map(|l| {
            let masked_count = l.iter().filter(|&&x| x == -100).count();
            let trainable_count = l.len() - masked_count;
            (masked_count, trainable_count)
        });

        debug!(
            "Parsed row: input_ids[0..5]={:?}, len={}, labels={}, position_ids={}, seq_lengths={}",
            &input_ids[..5.min(input_ids.len())],
            input_ids.len(),
            labels_info
                .map(|(m, t)| format!("masked={}, trainable={}", m, t))
                .unwrap_or("None".to_string()),
            position_ids
                .as_ref()
                .map(|p| format!("present, len={}", p.len()))
                .unwrap_or("None".to_string()),
            sequence_lengths
                .as_ref()
                .map(|s| format!("{:?}", s))
                .unwrap_or("None".to_string()),
        );

        Ok(TokenizedData {
            input_ids,
            labels,
            position_ids,
            sequence_lengths,
        })
    }

    fn parse_int_array(
        &self,
        value: &serde_json::Value,
        expected_len: Option<usize>,
        column_name: &str,
    ) -> Result<Vec<i32>> {
        let array = value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Column '{}' is not an array", column_name))?;

        let result: Result<Vec<i32>> = array
            .iter()
            .map(|v| {
                v.as_i64()
                    .ok_or_else(|| anyhow::anyhow!("Non-integer value in column '{}'", column_name))
                    .map(|i| i as i32)
            })
            .collect();

        let result = result?;

        if let Some(expected) = expected_len {
            if result.len() != expected {
                bail!(
                    "Column '{}' has length {} instead of expected {}",
                    column_name,
                    result.len(),
                    expected
                );
            }
        }

        Ok(result)
    }

    fn map_batch_id_to_rows(&self, batch_id: BatchId) -> Vec<usize> {
        let start = batch_id.0.start as usize;
        let end = batch_id.0.end as usize;

        if self.num_rows == 0 {
            return vec![];
        }

        let mut row_indices = Vec::new();
        for i in start..=end {
            let idx = i % self.num_rows;
            let actual_idx = if let Some(shuffle_indices) = &self.shuffle_indices {
                shuffle_indices[idx]
            } else {
                idx
            };
            row_indices.push(actual_idx);
        }

        row_indices
    }
}

impl TokenizedDataProvider for HuggingFacePreprocessedDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        if self.num_rows == 0 {
            bail!("No data available");
        }

        let row_indices = self.map_batch_id_to_rows(data_ids);

        // Batch consecutive indices together for efficient API calls
        let mut samples = Vec::with_capacity(row_indices.len());
        let mut i = 0;

        while i < row_indices.len() {
            let start_idx = row_indices[i];
            let mut length = 1;

            // Find consecutive indices (up to MAX_ROWS_PER_REQUEST)
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

        // If indices weren't consecutive, we may have fetched out of order
        // In that case, we need to reorder based on the requested indices
        if samples.len() != row_indices.len() {
            // Fall back to individual fetches for non-consecutive access
            samples.clear();
            for &row_idx in &row_indices {
                let row_samples = self.fetch_rows(row_idx, 1).await?;
                samples.extend(row_samples);
            }
        }

        Ok(samples)
    }
}

impl LengthKnownDataProvider for HuggingFacePreprocessedDataProvider {
    fn num_sequences(&self) -> usize {
        self.num_rows
    }
}
