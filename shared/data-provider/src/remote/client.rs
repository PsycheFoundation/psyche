use anyhow::{Result, bail};
use psyche_core::BatchId;
use psyche_network::{SecretKey, TcpClient};
use tracing::trace;

use crate::{TokenizedData, TokenizedDataProvider};

use super::shared::{ClientToServerMessage, ServerToClientMessage};

pub struct DataProviderTcpClient {
    address: String,
    tcp_client: TcpClient<ClientToServerMessage, ServerToClientMessage>,
}

impl DataProviderTcpClient {
    pub async fn connect(addr: String, secret_key: SecretKey) -> Result<Self> {
        let tcp_client =
            TcpClient::<ClientToServerMessage, ServerToClientMessage>::connect(&addr, secret_key)
                .await?;
        Ok(Self {
            tcp_client,
            address: addr.to_owned(),
        })
    }

    async fn receive_training_data(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        self.tcp_client
            .send(ClientToServerMessage::RequestTrainingData { data_ids })
            .await?;

        let message = self.tcp_client.receive().await?;
        match message {
            ServerToClientMessage::TrainingData {
                data_ids: received_id,
                raw_data,
            } => {
                if received_id == data_ids {
                    Ok(raw_data)
                } else {
                    bail!("Received data_id does not match requested data_id")
                }
            }
            e => bail!("Unexpected message from server {:?}", e),
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

impl TokenizedDataProvider for DataProviderTcpClient {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        trace!("[{:?}] get samples..", self.tcp_client.get_identity());
        self.receive_training_data(data_ids).await
    }
}
