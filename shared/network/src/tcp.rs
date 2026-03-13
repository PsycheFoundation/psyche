use crate::Networkable;

use anyhow::{anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use iroh::{PublicKey, SecretKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, io, marker::PhantomData, net::SocketAddr, sync::Arc};
use thiserror::Error;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{
        Mutex,
        mpsc::{self, error::SendError},
    },
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{debug, error, info};

use crate::SignedMessage;

const MAX_FRAME_LENGTH: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize, Debug)]
enum ServerToClientMessage<T: Debug> {
    Challenge([u8; 32]),
    Else(T),
}

#[derive(Serialize, Deserialize, Debug)]
enum ClientToServerMessage<T: Debug> {
    ChallengeResponse(Vec<u8>),
    Else(T),
}

pub enum ClientNotification<T: Debug, U: Debug> {
    Message(T),
    Disconnected(U),
}

pub struct TcpServer<ToServerMessage, ToClientMessage>
where
    ToServerMessage: Networkable + Debug + Send + Sync + 'static,
    ToClientMessage: Networkable + Debug + Send + Sync + 'static,
{
    clients: Arc<Mutex<HashMap<PublicKey, mpsc::UnboundedSender<ToClientMessage>>>>,
    _phantom: PhantomData<ToServerMessage>,

    incoming_msg_stream:
        tokio_stream::wrappers::UnboundedReceiverStream<(PublicKey, ToServerMessage)>,
    send_msg: mpsc::UnboundedSender<(PublicKey, ToClientMessage)>,
    local_addr: SocketAddr,
    disconnected_rx: mpsc::UnboundedReceiver<PublicKey>,
}

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("failed to bind to socket: {0}")]
    Bind(io::Error),
    #[error("failed to get local addr: {0}")]
    GetLocalAddr(io::Error),
}

impl<ToServer, ToClient> TcpServer<ToServer, ToClient>
where
    ToServer: Networkable + Clone + Debug + Send + Sync + 'static,
    ToClient: Networkable + Clone + Debug + Send + Sync + 'static,
{
    pub async fn start(addr: SocketAddr) -> Result<Self, ConnectError> {
        let listener = TcpListener::bind(addr).await.map_err(ConnectError::Bind)?;
        let local_addr = listener.local_addr().map_err(ConnectError::GetLocalAddr)?;
        info!("Server listening on: {}", local_addr);

        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let (send_msg, mut outgoing_rx) = mpsc::unbounded_channel();
        let (disconnected_tx, disconnected_rx) = mpsc::unbounded_channel();

        let clients = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn({
            let clients = clients.clone();
            async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let clients = clients.clone();
                    let incoming_tx = incoming_tx.clone();
                    let disconnected_tx = disconnected_tx.clone();
                    tokio::spawn(async move {
                        if let Err(err) =
                            Self::handle_connection(stream, clients, incoming_tx, disconnected_tx)
                                .await
                        {
                            error!("Error handling connection: {err:#}");
                        }
                    });
                }
            }
        });

        tokio::spawn({
            let clients = clients.clone();
            async move {
                while let Some((id, message)) = outgoing_rx.recv().await {
                    if let Some(client) = clients.lock().await.get(&id) {
                        if let Err(err) = client.send(message) {
                            error!("Failed to send message to client {id:?}: {err:#}");
                        }
                    }
                }
            }
        });

        Ok(Self {
            _phantom: Default::default(),
            clients,
            incoming_msg_stream: tokio_stream::wrappers::UnboundedReceiverStream::new(incoming_rx),
            send_msg,
            local_addr,
            disconnected_rx,
        })
    }

    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    async fn handle_connection(
        stream: TcpStream,
        clients: Arc<Mutex<HashMap<PublicKey, mpsc::UnboundedSender<ToClient>>>>,
        incoming_tx: mpsc::UnboundedSender<(PublicKey, ToServer)>,
        disconnected_tx: mpsc::UnboundedSender<PublicKey>,
    ) -> anyhow::Result<()> {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        // Generate and send challenge
        let mut challenge = [0u8; 32];
        rand::rng().fill_bytes(&mut challenge);
        framed
            .send(
                ServerToClientMessage::<ToClient>::Challenge(challenge)
                    .to_bytes()
                    .into(),
            )
            .await?;
        debug!("New client joined - sent challenge {:?}", challenge);

        // Receive and verify challenge response
        let response = ClientToServerMessage::<ToClient>::from_bytes(
            &framed
                .next()
                .await
                .ok_or_else(|| anyhow!("No response received"))??,
        )?;
        let challenge_response = if let ClientToServerMessage::ChallengeResponse(res) = response {
            res
        } else {
            bail!(
                "Invalid client-to-server message - expected ChallengeResponse, got {:?}",
                response
            );
        };
        debug!("Got response for challenge {:?}", challenge);
        let (identity, decoded_challenge) =
            SignedMessage::<[u8; 32]>::verify_and_decode(&challenge_response)?;
        if decoded_challenge != challenge {
            bail!(
                "Challenge doesn't match: {:?} != {:?}",
                decoded_challenge,
                challenge
            );
        }
        debug!("Challenge response accepted! welcome, {:?}!", identity);
        let (client_tx, mut client_rx) = mpsc::unbounded_channel();
        clients.lock().await.insert(identity, client_tx);

        loop {
            tokio::select! {
                Some(message) = client_rx.recv() => {
                    framed.send(ServerToClientMessage::Else(message).to_bytes().into()).await?;
                }
                result = framed.next() => match result {
                    Some(Ok(bytes)) => {
                        let message = ClientToServerMessage::<ToServer>::from_bytes(&bytes)?;
                        match message {
                            ClientToServerMessage::ChallengeResponse(..) => {
                               bail!("Unexpected challenge message");
                            }
                            ClientToServerMessage::Else(m) => {
                                incoming_tx.send((identity, m))?;
                            }
                        }
                    }
                    Some(Err(err)) => {
                        error!("Error reading from stream: {err:#}");
                        break;
                    }
                    None => break,
                },
            }
        }

        clients.lock().await.remove(&identity);
        disconnected_tx.send(identity)?;
        Ok(())
    }

    pub async fn get_connected_clients(&self) -> Vec<PublicKey> {
        self.clients.lock().await.keys().cloned().collect()
    }

    pub async fn next(&mut self) -> Option<ClientNotification<(PublicKey, ToServer), PublicKey>> {
        select! {
            Some(msg) = self.incoming_msg_stream.next() => {
                Some(ClientNotification::Message(msg))
            }
            Some(msg) = self.disconnected_rx.recv() => {
                Some(ClientNotification::Disconnected(msg))
            }
            else => None
        }
    }

    pub async fn send_to(
        &mut self,
        to: PublicKey,
        msg: ToClient,
    ) -> Result<(), SendError<(PublicKey, ToClient)>> {
        self.send_msg.send((to, msg))
    }

    pub async fn broadcast(
        &mut self,
        msg: ToClient,
    ) -> Result<(), SendError<(PublicKey, ToClient)>> {
        let clients = self.get_connected_clients().await;
        let v: Result<Vec<()>, _> = clients
            .into_iter()
            .map(|client| self.send_msg.send((client, msg.clone())))
            .collect();
        v?;
        Ok(())
    }
}

pub struct TcpClient<ToServerMessage, ToClientMessage>
where
    ToServerMessage: Networkable + Debug + Send + Sync + 'static,
    ToClientMessage: Networkable + Debug + Send + Sync + 'static,
{
    identity: PublicKey,
    framed: Framed<TcpStream, LengthDelimitedCodec>,
    _phantom: PhantomData<(ToServerMessage, ToClientMessage)>,
}

impl<ToServer, ToClient> TcpClient<ToServer, ToClient>
where
    ToServer: Networkable + Debug + Send + Sync + 'static,
    ToClient: Networkable + Debug + Send + Sync + 'static,
{
    pub async fn connect(addr: &str, secret_key: SecretKey) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to server at: {}", addr);

        let mut codec = LengthDelimitedCodec::new();
        codec.set_max_frame_length(MAX_FRAME_LENGTH);
        let mut framed = Framed::new(stream, codec);

        // Receive challenge
        let challenge = match Self::receive_message(&mut framed).await? {
            ServerToClientMessage::Challenge(c) => c,
            _ => return Err(anyhow!("Expected challenge, got something else")),
        };

        // Sign and send challenge response
        let response = SignedMessage::<[u8; 32]>::sign_and_encode(&secret_key, &challenge)?;
        framed
            .send(
                ClientToServerMessage::<ToServer>::ChallengeResponse(response.to_vec())
                    .to_bytes()
                    .into(),
            )
            .await?;

        Ok(Self {
            identity: secret_key.public(),
            framed,
            _phantom: Default::default(),
        })
    }

    async fn receive_message(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    ) -> anyhow::Result<ServerToClientMessage<ToClient>> {
        let bytes = framed
            .next()
            .await
            .ok_or_else(|| anyhow!("Connection closed"))??;
        ServerToClientMessage::from_bytes(&bytes)
    }

    pub async fn send(&mut self, message: ToServer) -> anyhow::Result<()> {
        Ok(self
            .framed
            .send(ClientToServerMessage::Else(message).to_bytes().into())
            .await?)
    }

    /// # Cancel safety
    ///
    /// This method is cancel safe. If `receive` is used as the event in a
    /// [`tokio::select!`](crate::select) statement and some other branch
    /// completes first, it is guaranteed that no messages were received.
    pub async fn receive(&mut self) -> anyhow::Result<ToClient> {
        match Self::receive_message(&mut self.framed).await? {
            ServerToClientMessage::Else(message) => Ok(message),
            // TODO errors here
            ServerToClientMessage::Challenge(_) => Err(anyhow!("Unexpected challenge message")),
        }
    }

    pub fn get_identity(&self) -> &PublicKey {
        &self.identity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    enum TestToServer {
        Ping(String),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    enum TestToClient {
        Pong(String),
    }

    /// the TCP handshake validates clients with their iroh SecretKey/PublicKey
    /// challenge/response.
    #[tokio::test]
    async fn test_tcp_handshake_uses_only_iroh_key() {
        let server = TcpServer::<TestToServer, TestToClient>::start("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = server.local_addr().to_string();

        // connect with just an iroh SecretKey
        let secret_key = SecretKey::generate(&mut rand::rng());
        let expected_public = secret_key.public();

        let client = TcpClient::<TestToServer, TestToClient>::connect(&addr, secret_key)
            .await
            .unwrap();

        // server identifies the client by its PublicKey
        assert_eq!(*client.get_identity(), expected_public);

        // server should see the client in connected list
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let connected = server.get_connected_clients().await;
        assert!(
            connected.contains(&expected_public),
            "Server should identify client by iroh PublicKey"
        );
    }

    /// multiple clients can connect
    #[tokio::test]
    async fn test_multiple_clients() {
        let server = TcpServer::<TestToServer, TestToClient>::start("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = server.local_addr().to_string();

        let key1 = SecretKey::generate(&mut rand::rng());
        let key2 = SecretKey::generate(&mut rand::rng());
        let pub1 = key1.public();
        let pub2 = key2.public();

        let _client1 = TcpClient::<TestToServer, TestToClient>::connect(&addr, key1)
            .await
            .unwrap();
        let _client2 = TcpClient::<TestToServer, TestToClient>::connect(&addr, key2)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let connected = server.get_connected_clients().await;
        assert!(connected.contains(&pub1));
        assert!(connected.contains(&pub2));
        assert_eq!(connected.len(), 2);
    }

    /// client using the wrong key to sign the challenge are kicked
    #[tokio::test]
    async fn test_challenge_rejects_invalid_signature() {
        let server = TcpServer::<TestToServer, TestToClient>::start("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = server.local_addr().to_string();

        // manually connect and send a bad challenge response
        let stream = TcpStream::connect(&addr).await.unwrap();
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        // get the challenge
        let bytes = framed.next().await.unwrap().unwrap();
        let msg = ServerToClientMessage::<TestToClient>::from_bytes(&bytes).unwrap();
        let _challenge = match msg {
            ServerToClientMessage::Challenge(c) => c,
            _ => panic!("Expected challenge"),
        };

        // sign a different challenge
        let secret_key = SecretKey::generate(&mut rand::rng());
        let wrong_challenge = [0xFFu8; 32];
        let bad_response =
            SignedMessage::<[u8; 32]>::sign_and_encode(&secret_key, &wrong_challenge).unwrap();
        framed
            .send(
                ClientToServerMessage::<TestToServer>::ChallengeResponse(bad_response.to_vec())
                    .to_bytes()
                    .into(),
            )
            .await
            .unwrap();

        // server rejects this client
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let connected = server.get_connected_clients().await;
        assert!(
            connected.is_empty(),
            "Client with wrong challenge response should be rejected"
        );
    }

    #[tokio::test]
    async fn test_message_exchange_after_iroh_handshake() {
        let mut server =
            TcpServer::<TestToServer, TestToClient>::start("127.0.0.1:0".parse().unwrap())
                .await
                .unwrap();
        let addr = server.local_addr().to_string();

        let secret_key = SecretKey::generate(&mut rand::rng());
        let pub_key = secret_key.public();
        let mut client = TcpClient::<TestToServer, TestToClient>::connect(&addr, secret_key)
            .await
            .unwrap();

        client
            .send(TestToServer::Ping("hiiii".into()))
            .await
            .unwrap();

        let notification = server.next().await.unwrap();
        match notification {
            ClientNotification::Message((from, TestToServer::Ping(text))) => {
                assert_eq!(from, pub_key);
                assert_eq!(text, "hiiii");
            }
            _ => panic!("Expected message from client"),
        }

        server
            .send_to(pub_key, TestToClient::Pong("hewwo :3".into()))
            .await
            .unwrap();

        let reply = client.receive().await.unwrap();
        match reply {
            TestToClient::Pong(text) => assert_eq!(text, "hewwo :3"),
        }
    }
}
