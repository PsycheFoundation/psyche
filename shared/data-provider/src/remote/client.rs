use anyhow::{Result, bail};
use psyche_core::BatchId;
use psyche_network::{AuthenticatableIdentity, TcpClient};
use tracing::trace;

use crate::{TokenizedData, TokenizedDataProvider};

use super::shared::{ClientToServerMessage, ServerToClientMessage};

pub struct DataProviderTcpClient<T: AuthenticatableIdentity> {
    address: String,
    tcp_client: TcpClient<T, ClientToServerMessage, ServerToClientMessage>,
}

impl<T: AuthenticatableIdentity> DataProviderTcpClient<T> {
    pub async fn connect(addr: String, identity: T, private_key: T::PrivateKey) -> Result<Self> {
        let tcp_client = TcpClient::<T, ClientToServerMessage, ServerToClientMessage>::connect(
            &addr,
            identity,
            private_key,
        )
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

impl<T: AuthenticatableIdentity> TokenizedDataProvider for DataProviderTcpClient<T> {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        trace!("[{:?}] get samples..", self.tcp_client.get_identity());
        self.receive_training_data(data_ids).await
    }
}
