use anchor_lang::prelude::*;
use anyhow::{Result, bail};
use async_trait::async_trait;
use bytemuck::Zeroable;
use futures::future::try_join_all;
use parquet::data_type::AsBytes;
use psyche_coordinator::{Coordinator, HealthChecks, model::Checkpoint};
use psyche_core::{BatchId, NodeIdentity};
use psyche_data_provider::{
    DataProviderTcpClient, DataProviderTcpServer, LengthKnownDataProvider, TokenizedData,
    TokenizedDataProvider,
};
use psyche_network::{AuthenticatableIdentity, FromSignedBytesError, Networkable};
use psyche_tui::logging;
use psyche_watcher::{Backend as WatcherBackend, OpportunisticData};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tracing::info;
use ts_rs::TS;

// Simulated backend for demonstration
#[allow(dead_code)]
struct DummyBackend<T: NodeIdentity>(Vec<T>);

#[async_trait]
impl<T: NodeIdentity> WatcherBackend<T> for DummyBackend<T> {
    async fn wait_for_new_state(&mut self) -> anyhow::Result<Coordinator<T>> {
        Ok(Coordinator::zeroed())
    }

    async fn send_witness(&mut self, _opportunistic_data: OpportunisticData) -> anyhow::Result<()> {
        bail!("Data provider does not send witnesses");
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks<T>) -> anyhow::Result<()> {
        bail!("Data provider does not send health check");
    }

    async fn send_checkpoint(&mut self, _checkpoint: Checkpoint) -> anyhow::Result<()> {
        bail!("Data provider does not send checkpoints");
    }
}

#[derive(
    Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq, Default, Copy, Zeroable, TS,
)]
struct DummyNodeIdentity(u64);

impl Display for DummyNodeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))?;
        Ok(())
    }
}
impl NodeIdentity for DummyNodeIdentity {
    fn get_p2p_public_key(&self) -> &[u8; 32] {
        todo!()
    }
}

impl AuthenticatableIdentity for DummyNodeIdentity {
    type PrivateKey = ();

    fn from_signed_challenge_bytes(
        bytes: &[u8],
        challenge: [u8; 32],
    ) -> Result<Self, FromSignedBytesError> {
        let (serialized_challenge, bytes) = bytes.split_at(32);
        if challenge != serialized_challenge {
            return Err(FromSignedBytesError::MismatchedChallenge(
                challenge,
                serialized_challenge.into(),
            ));
        }
        Self::from_bytes(bytes).map_err(|_| FromSignedBytesError::Deserialize)
    }

    fn to_signed_challenge_bytes(&self, _private_key: &(), challenge: [u8; 32]) -> Vec<u8> {
        let mut b = challenge.to_vec();
        b.extend(self.to_bytes());
        b
    }

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        todo!()
    }

    fn raw_p2p_sign(&self, _private_key: &Self::PrivateKey, _bytes: &[u8]) -> [u8; 64] {
        todo!()
    }
}

impl AsRef<[u8]> for DummyNodeIdentity {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl AnchorSerialize for DummyNodeIdentity {
    fn serialize<W: std::io::Write>(&self, _: &mut W) -> std::io::Result<()> {
        unimplemented!()
    }
}

impl AnchorDeserialize for DummyNodeIdentity {
    fn deserialize_reader<R: std::io::Read>(_: &mut R) -> std::io::Result<Self> {
        unimplemented!()
    }
}

impl anchor_lang::Space for DummyNodeIdentity {
    const INIT_SPACE: usize = 0;
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

    let clients: Vec<_> = (0..4).map(DummyNodeIdentity).collect();
    let backend = DummyBackend(clients.clone());

    tokio::spawn(async move {
        let local_data_provider = DummyDataProvider;
        let mut server = DataProviderTcpServer::<_, DummyNodeIdentity, _, _>::start(
            local_data_provider,
            backend,
            5740,
        )
        .await
        .unwrap();
        loop {
            server.poll().await;
        }
    });

    let mut clients = try_join_all(
        clients
            .into_iter()
            .map(|i| DataProviderTcpClient::connect("localhost:5740".to_string(), i, ())),
    )
    .await?;
    info!("clients initialized successfully");
    loop {
        for (i, c) in clients.iter_mut().enumerate() {
            c.get_samples(BatchId((0, 0).into())).await?;
            info!("client {} got data! ", i);
        }
    }
}
