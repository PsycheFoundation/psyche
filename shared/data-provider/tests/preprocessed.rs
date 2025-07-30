use std::{path::PathBuf, time::Instant};

use pretty_assertions::assert_eq;
use psyche_core::{BatchId, Shuffle};
use psyche_data_provider::{PreprocessedDataProvider, Split, TokenizedDataProvider};
use serde::Deserialize;
use tokio::fs::read_to_string;

fn test_path(path: &[&str]) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "tests"]
        .iter()
        .chain(path)
        .collect()
}

const SEED: [u8; 32] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32,
];

#[derive(Deserialize)]
struct DecodedData {
    pub inputs: Vec<i32>,
    pub labels: Option<Vec<i32>>,
    pub position_ids: Option<Vec<i32>>,
    pub sequence_lengths: Option<Vec<i32>>,
}

#[tokio::test]
async fn loads_hermes3_subset() {
    let data_dir = test_path(&[
        "Hermes-3-Preprocessed-Llama3",
        "snapshots",
        "1e1b1bc828e0fb5e9f869189f381f68849854303",
        "data",
    ]);
    let start_timer = Instant::now();
    let mut data_loader = PreprocessedDataProvider::new_from_directory(
        data_dir,
        4096,
        Shuffle::Seeded(SEED),
        Some(Split::Train),
        None,
    )
    .unwrap();
    let elapsed = start_timer.elapsed();
    println!("DATA LOADING TOOK {}", elapsed.as_secs());

    let samples = data_loader
        .get_samples(BatchId((0, 1).into()))
        .await
        .unwrap();
    for (i, sample) in samples.into_iter().enumerate() {
        let decoded_path = test_path(&["resources", "hermes3", "decoded", &format!("{i}.json")]);
        let expected = read_to_string(&decoded_path)
            .await
            .unwrap_or_else(|_| panic!("no decoded file at {decoded_path:?}"));
        let expected: DecodedData = serde_json::from_str(&expected).unwrap();

        assert_eq!(
            sample.input_ids, expected.inputs,
            "sample `inputs` {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
        assert_eq!(
            sample.labels, expected.labels,
            "sample `labels` {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
        assert_eq!(
            sample.position_ids, expected.position_ids,
            "sample `position_ids` {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
        assert_eq!(
            sample.sequence_lengths, expected.sequence_lengths,
            "sample `sequence_lengths` {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
    }
}
