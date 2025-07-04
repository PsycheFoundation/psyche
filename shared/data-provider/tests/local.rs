use std::path::PathBuf;

use pretty_assertions::assert_eq;
use psyche_core::{BatchId, Shuffle, TokenSize};
use psyche_data_provider::{LocalDataProvider, TokenizedDataProvider};
use tokenizers::Tokenizer;
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

#[tokio::test]
async fn loads_dolma_subset() {
    let data_dir = test_path(&["resources", "dolma", "data"]);
    let mut data_loader = LocalDataProvider::new_from_directory(
        data_dir,
        TokenSize::TwoBytes,
        2048,
        Shuffle::Seeded(SEED),
    )
    .unwrap();
    let samples = data_loader
        .get_samples(BatchId((0, 1).into()))
        .await
        .unwrap();

    let tokenizer = Tokenizer::from_file(test_path(&["resources", "llama2_tokenizer.json"]))
        .expect("tokenizer json exists");
    for (i, sample) in samples.into_iter().enumerate() {
        let decoded_path = test_path(&["resources", "dolma", "decoded", &format!("{i}.txt")]);

        let expected = read_to_string(&decoded_path)
            .await
            .unwrap_or_else(|_| panic!("no decoded file at {decoded_path:?}"));

        let decoded = tokenizer
            .decode(
                &sample.into_iter().map(|x| x as u32).collect::<Vec<_>>(),
                true,
            )
            .unwrap();

        assert_eq!(
            decoded, expected,
            "sample {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
    }
}

#[tokio::test]
async fn loads_fineweb_subset() {
    let data_dir = test_path(&["resources", "fineweb", "data"]);
    let mut data_loader = LocalDataProvider::new_from_directory(
        data_dir,
        TokenSize::TwoBytes,
        2048,
        Shuffle::Seeded(SEED),
    )
    .unwrap();
    let samples = data_loader
        .get_samples(BatchId((0, 1).into()))
        .await
        .unwrap();

    let tokenizer = Tokenizer::from_file(test_path(&["resources", "llama2_tokenizer.json"]))
        .expect("tokenizer json exists");
    for (i, sample) in samples.into_iter().enumerate() {
        let decoded_path = test_path(&["resources", "fineweb", "decoded", &format!("{i}.txt")]);

        let expected = read_to_string(&decoded_path)
            .await
            .unwrap_or_else(|_| panic!("no decoded file at {decoded_path:?}"));

        let decoded = tokenizer
            .decode(
                &sample.into_iter().map(|x| x as u32).collect::<Vec<_>>(),
                true,
            )
            .unwrap();

        assert_eq!(
            decoded, expected,
            "sample {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
    }
}
