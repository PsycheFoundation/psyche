use anyhow::Result;
use bytemuck::Zeroable;
use psyche_coordinator::coordinator::Coordinator;
use psyche_core::BatchId;
use psyche_network::{ClientNotification, PublicKey, TcpServer};
use psyche_watcher::Backend;
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

use crate::{
    TokenizedData,
    traits::{LengthKnownDataProvider, TokenizedDataProvider},
};

use super::shared::{ClientToServerMessage, RejectionReason, ServerToClientMessage};

pub struct DataProviderTcpServer<D, W>
where
    D: TokenizedDataProvider + LengthKnownDataProvider,
    W: Backend,
{
    tcp_server: TcpServer<ClientToServerMessage, ServerToClientMessage>,
    pub(crate) local_data_provider: D,
    backend: W,
    pub(crate) state: Coordinator,
    pub(crate) in_round: HashSet<[u8; 32]>,
    pub(crate) provided_sequences: HashMap<PublicKey, usize>,
}

impl<D, W> DataProviderTcpServer<D, W>
where
    D: TokenizedDataProvider + LengthKnownDataProvider + 'static,
    W: Backend + 'static,
{
    pub async fn start(local_data_provider: D, backend: W, port: u16) -> Result<Self> {
        let tcp_server = TcpServer::<ClientToServerMessage, ServerToClientMessage>::start(
            format!("0.0.0.0:{port}").parse()?,
        )
        .await?;
        Ok(DataProviderTcpServer {
            tcp_server,
            local_data_provider,
            in_round: HashSet::new(),
            provided_sequences: HashMap::new(),
            backend,
            state: Coordinator::zeroed(),
        })
    }

    pub async fn poll(&mut self) {
        tokio::select! {
            new_state = self.backend.wait_for_new_state() => {
                self.handle_new_state(new_state.unwrap());
            }
            Some(event) = self.tcp_server.next() => {
                match event {
                    ClientNotification::Message((from, message)) => {
                        self.handle_client_message(from, message).await;
                    }
                    ClientNotification::Disconnected(_) => {
                        // noop :)
                    }
                }
            }
        }
    }
    pub async fn handle_client_message(&mut self, from: PublicKey, message: ClientToServerMessage) {
        match message {
            ClientToServerMessage::RequestTrainingData { data_ids } => {
                let result = self.try_send_data(from, data_ids).await;
                match result {
                    Ok(data) => {
                        let old_count = *self.provided_sequences.get(&from).unwrap_or(&0);
                        self.provided_sequences
                            .insert(from, old_count + data_ids.len());
                        match self
                            .tcp_server
                            .send_to(
                                from,
                                ServerToClientMessage::TrainingData {
                                    data_ids,
                                    raw_data: data,
                                },
                            )
                            .await
                        {
                            Ok(()) => {
                                debug!("sent training data to {:?}", from);
                            }
                            Err(err) => {
                                warn!("Failed to send training data to {:?}: {err}", from);
                            }
                        }
                    }
                    Err(reason) => {
                        match self
                            .tcp_server
                            .send_to(
                                from,
                                ServerToClientMessage::RequestRejected { data_ids, reason },
                            )
                            .await
                        {
                            Ok(()) => {
                                debug!("sent error to {:?}", from);
                            }
                            Err(err) => {
                                warn!("Failed to send error to {:?}: {err}", from);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn try_send_data(
        &mut self,
        to: PublicKey,
        data_ids: BatchId,
    ) -> Result<Vec<TokenizedData>, RejectionReason> {
        if !self.in_round.contains(to.as_bytes()) {
            return Err(RejectionReason::NotInThisRound);
        }

        let data = self
            .local_data_provider
            .get_samples(data_ids)
            .await
            .expect("data failed to fetch...");
        Ok(data)
    }

    fn handle_new_state(&mut self, state: Coordinator) {
        self.state = state;
        self.in_round = self
            .state
            .epoch_state
            .clients
            .iter()
            .map(|x| *x.id.p2p_identity())
            .collect();
    }
}
