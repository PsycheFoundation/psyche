use anyhow::Result;
use psyche_core::{BatchId, Shuffle, TokenSize};
use psyche_data_provider::{
    TokenizedDataProvider,
    http::{FileURLs, HttpDataProvider},
};
use std::io::Write;
use std::net::SocketAddr;
use std::{fs::File, time::Duration};
use test_log::test;
use tokio::time::timeout;
use tracing::debug;

struct TestServer {
    cancel: tokio::sync::watch::Sender<()>,
    addr: SocketAddr,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.cancel.send(()).unwrap();
    }
}

impl TestServer {
    async fn new(files: Vec<Vec<u8>>) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;

        for (idx, data) in files.iter().enumerate() {
            let file_path = temp_dir.path().join(format!("{idx:0>3}.ds"));
            let mut file = File::create(&file_path)?;
            file.write_all(data)?;
            debug!("created temp test file {file_path:?}");
        }

        let (cancel, rx_cancel) = tokio::sync::watch::channel(());
        let mut settings = static_web_server::Settings::get_unparsed(false)?;
        settings.general.port = 0;
        settings.general.root = temp_dir.keep();
        let (tx_port, rx_port) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            static_web_server::Server::new(settings)
                .unwrap()
                .run_standalone(Some(rx_cancel), tx_port)
                .unwrap();
        });
        let port = rx_port.await?;
        let addr = SocketAddr::new("127.0.0.1".parse()?, port);
        Ok(Self { addr, cancel })
    }
}

#[test(tokio::test)]
async fn test_http_data_provider() -> Result<()> {
    const FILE_SIZE: u64 = 16;
    const SEQUENCE_LEN: u32 = 3;

    let file1: Vec<u8> = (0..FILE_SIZE).map(|i| i as u8).collect();
    let file2: Vec<u8> = (FILE_SIZE..FILE_SIZE * 2).map(|i| i as u8).collect();

    let server = TestServer::new(vec![file1.clone(), file2.clone()]).await?;
    let base_url = format!("http://{}/{{}}.ds", server.addr);

    let mut provider = HttpDataProvider::new(
        timeout(
            Duration::from_secs(2),
            FileURLs::from_template(&base_url, 0, 3, 2),
        )
        .await??,
        TokenSize::TwoBytes,
        SEQUENCE_LEN,
        Shuffle::DontShuffle,
    )?;

    // Test first sequence
    println!("first sequence..");
    let samples = timeout(
        Duration::from_secs(2),
        provider.get_samples(BatchId((0, 0).into())),
    )
    .await??;

    assert_eq!(samples.len(), 1);
    let first_sequence = &samples[0].input_ids;

    let expected_sequence: Vec<i32> = vec![
        i32::from_le_bytes([0, 1, 0, 0]),
        i32::from_le_bytes([2, 3, 0, 0]),
        i32::from_le_bytes([4, 5, 0, 0]),
    ];

    assert_eq!(first_sequence, &expected_sequence);

    // Test second sequence (last sequence of first file)
    println!("second sequence..");
    let last_sequence_first_file = timeout(
        Duration::from_secs(5),
        provider.get_samples(BatchId((1, 1).into())),
    )
    .await??;

    let expected_last_sequence: Vec<i32> = vec![
        i32::from_le_bytes([6, 7, 0, 0]),
        i32::from_le_bytes([8, 9, 0, 0]),
        i32::from_le_bytes([10, 11, 0, 0]),
    ];

    assert_eq!(
        last_sequence_first_file[0].input_ids,
        expected_last_sequence
    );

    Ok(())
}

#[test(tokio::test)]
async fn test_http_data_provider_shuffled() -> Result<()> {
    const FILE_SIZE: u64 = 16;
    const SEQUENCE_LEN: u32 = 3;

    let file1: Vec<u8> = (0..FILE_SIZE).map(|i| i as u8).collect();
    let file2: Vec<u8> = (FILE_SIZE..FILE_SIZE * 2).map(|i| i as u8).collect();

    let server = TestServer::new(vec![file1.clone(), file2.clone()]).await?;
    let base_url = format!("http://{}/{{}}.ds", server.addr);

    let seed = [42u8; 32];

    let mut provider = HttpDataProvider::new(
        timeout(
            Duration::from_secs(2),
            FileURLs::from_template(&base_url, 0, 3, 2),
        )
        .await??,
        TokenSize::TwoBytes,
        SEQUENCE_LEN,
        Shuffle::Seeded(seed),
    )?;

    let batch_id = BatchId((0, 0).into());

    // Test first sequence with first provider
    let samples = timeout(Duration::from_secs(2), provider.get_samples(batch_id)).await??;

    // Create second provider with same seed
    let mut provider2 = HttpDataProvider::new(
        timeout(
            Duration::from_secs(2),
            FileURLs::from_template(&base_url, 0, 3, 2),
        )
        .await??,
        TokenSize::TwoBytes,
        SEQUENCE_LEN,
        Shuffle::Seeded(seed),
    )?;

    // Test first sequence with second provider
    let samples2 = timeout(Duration::from_secs(2), provider2.get_samples(batch_id)).await??;

    // Sequences should be equal when using same seed
    assert_eq!(samples, samples2);

    // Create third provider without shuffle
    let mut provider3 = HttpDataProvider::new(
        timeout(
            Duration::from_secs(2),
            FileURLs::from_template(&base_url, 0, 3, 2),
        )
        .await??,
        TokenSize::TwoBytes,
        SEQUENCE_LEN,
        Shuffle::DontShuffle,
    )?;

    // Test first sequence with third provider
    let samples3 = timeout(Duration::from_secs(2), provider3.get_samples(batch_id)).await??;

    // Sequences should be different between shuffled and non-shuffled
    assert_ne!(samples, samples3);

    Ok(())
}
