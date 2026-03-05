use anyhow::{Result, bail};
use async_trait::async_trait;
use bytemuck::Zeroable;
use futures::future::try_join_all;
use parquet::data_type::AsBytes;
use psyche_coordinator::{Coordinator, HealthChecks, model::Checkpoint};
use psyche_core::BatchId;
use psyche_data_provider::{
    DataProviderTcpClient, DataProviderTcpServer, LengthKnownDataProvider, TokenizedData,
    TokenizedDataProvider,
};
use psyche_network::SecretKey;
use psyche_tui::logging;
use psyche_watcher::{Backend as WatcherBackend, OpportunisticData};
use rand::Rng;
use tracing::info;

// Simulated backend for demonstration
#[allow(dead_code)]
struct DummyBackend;

#[async_trait]
impl WatcherBackend for DummyBackend {
    async fn wait_for_new_state(&mut self) -> anyhow::Result<Coordinator> {
        Ok(Coordinator::zeroed())
    }

    async fn send_witness(&mut self, _opportunistic_data: OpportunisticData) -> anyhow::Result<()> {
        bail!("Data provider does not send witnesses");
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> anyhow::Result<()> {
        bail!("Data provider does not send health check");
    }

    async fn send_checkpoint(&mut self, _checkpoint: Checkpoint) -> anyhow::Result<()> {
        bail!("Data provider does not send checkpoints");
    }
}

struct DummyDataProvider;
impl TokenizedDataProvider for DummyDataProvider {
    async fn get_samples(&mut self, _data_ids: BatchId) -> anyhow::Result<Vec<TokenizedData>> {
        let mut data: [i32; 1024] = [0; 1024];
        rand::rng().fill(&mut data);
        Ok(vec![TokenizedData::from_input_ids(data.to_vec())])
    }
}

impl LengthKnownDataProvider for DummyDataProvider {
    fn num_sequences(&self) -> usize {
        usize::MAX
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = logging().init()?;

    let num_clients = 4;
    let backend = DummyBackend;

    tokio::spawn(async move {
        let local_data_provider = DummyDataProvider;
        let mut server = DataProviderTcpServer::start(local_data_provider, backend, 5740)
            .await
            .unwrap();
        loop {
            server.poll().await;
        }
    });

    let mut clients = try_join_all((0..num_clients).map(|_| {
        let secret_key = SecretKey::generate(&mut rand::rng());
        DataProviderTcpClient::connect("localhost:5740".to_string(), secret_key)
    }))
    .await?;
    info!("clients initialized successfully");
    loop {
        for (i, c) in clients.iter_mut().enumerate() {
            c.get_samples(BatchId((0, 0).into())).await?;
            info!("client {} got data! ", i);
        }
    }
}
