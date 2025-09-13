use anyhow::Result;
use ed25519::pkcs8::spki::der::pem::decode_label;
use iroh::{endpoint::Connection, protocol::ProtocolHandler};
use iroh::{NodeAddr, NodeId};
use iroh_blobs::{ticket::BlobTicket, Hash};
use psyche_core::BoxedFuture;
use serde::{de, ser, Deserialize, Serialize};
use std::collections::{btree_map::Entry, HashMap, HashSet};
use std::collections::{BTreeMap, VecDeque};
use std::io::{Cursor, Write};
use std::ops::{Deref, DerefMut};
use tch::Tensor;
use thiserror::Error;
use tokenizers::Tokenizer;
use tokio::sync::{
    mpsc::{self, UnboundedSender},
    oneshot,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::{NetworkConnection, Networkable, TransmittableDownload};

#[derive(Debug)]
/// Manager for the list of peers to ask for the model parameters and config
pub struct PeerManagerHandle {
    peer_tx: mpsc::UnboundedSender<PeerCommand>,
}

#[derive(Debug)]
/// List of commands that the Peer manager actor will respond in the process of asking and downloading the model parameters
enum PeerCommand {
    SetPeers {
        peers: Vec<NodeId>,
    },
    GetPeer {
        reply: oneshot::Sender<Option<NodeId>>,
    },
    ReportSuccess {
        peer_id: NodeId,
    },
    ReportModelDownloadError {
        blob_ticket: Option<BlobTicket>,
        peer_id: NodeId,
    },
}

impl PeerManagerHandle {
    pub fn new(max_errors_per_peer: u8, cancellation_token: CancellationToken) -> Self {
        let (peer_tx, peer_rx) = mpsc::unbounded_channel();

        // Spawn the peer manager actor
        tokio::spawn(peer_manager_actor(
            peer_rx,
            max_errors_per_peer,
            cancellation_token,
        ));

        Self { peer_tx }
    }

    /// Set the list of peers that the manager will use to download the model parameters
    pub fn set_peers(&self, peers: Vec<NodeId>) {
        let _ = self.peer_tx.send(PeerCommand::SetPeers { peers });
    }

    /// Get the next peer to download the model parameters from
    /// We'll get a None if no peers are available, a peer might be available later when it finishes sharing a parameter
    pub async fn get_next_peer(&self) -> Option<NodeId> {
        let (reply_tx, reply_rx) = oneshot::channel();

        if self
            .peer_tx
            .send(PeerCommand::GetPeer { reply: reply_tx })
            .is_err()
        {
            return None; // Manager actor is dead
        }

        reply_rx.await.unwrap_or(None)
    }

    /// Report that a peer has successfully shared the hash of a blob ticket for a parameter
    pub fn report_success(&self, peer_id: NodeId) {
        let _ = self.peer_tx.send(PeerCommand::ReportSuccess { peer_id });
    }

    /// Report that a peer has failed to share the hash of the blob ticket for a model parameter
    pub fn report_blob_ticket_request_error(
        &self,
        peer_id: NodeId,
        blob_ticket: Option<BlobTicket>,
    ) {
        if self
            .peer_tx
            .send(PeerCommand::ReportModelDownloadError {
                peer_id,
                blob_ticket,
            })
            .is_err()
        {
            tracing::error!("Failed to report error for peer {peer_id}, PeerManager actor is dead");
        }
    }
}

struct PeerManagerActor {
    /// Peers that are available to request the model to
    available_peers: VecDeque<NodeId>,
    /// A map for the peer's blob ticket to their errors
    errors_per_peers: HashMap<NodeId, u8>,
    /// Max errors we tolerate for a peer to share a parameter blob ticket
    max_errors_per_peer: u8,
}

impl PeerManagerActor {
    pub fn new(max_errors_per_peer: u8) -> Self {
        Self {
            available_peers: VecDeque::new(),
            errors_per_peers: HashMap::new(),
            max_errors_per_peer,
        }
    }

    fn handle_message(&mut self, message: PeerCommand, cancellation_token: CancellationToken) {
        match message {
            PeerCommand::SetPeers { peers } => {
                self.available_peers = VecDeque::from(peers);
                let errors_per_peers_vec = self.available_peers.iter().map(|peer| (*peer, 0_u8));
                self.errors_per_peers = HashMap::from_iter(errors_per_peers_vec);

                info!(
                    "Updated peer list: {} peers available to ask for the model parameters",
                    self.available_peers.len()
                );
            }
            PeerCommand::GetPeer { reply } => {
                let peer = if let Some(peer) = self.available_peers.pop_front() {
                    info!("Selected peer {peer} to ask for the model parameters");
                    Some(peer)
                } else {
                    info!("No available peers to ask for the model parameters at the moment");
                    None
                };
                let _ = reply.send(peer);
            }
            PeerCommand::ReportSuccess { peer_id } => {
                if !self.available_peers.contains(&peer_id) {
                    self.available_peers.push_back(peer_id);
                } else {
                    warn!("Peer was already available but we tried to add it again");
                }
                info!("Peer {peer_id} correctly provided the blob ticket");
            }
            PeerCommand::ReportModelDownloadError {
                peer_id,
                blob_ticket,
            } => {
                let error_count = self.errors_per_peers.entry(peer_id).or_insert(0);
                *error_count += 1;

                warn!(
                    "Error requesting a blob ticket {:?} from peer {peer_id}, it already failed {} time(s)",
                    blob_ticket.map(|bl| bl.hash()),
                    error_count
                );
                if *error_count >= self.max_errors_per_peer {
                    self.available_peers.retain(|id| *id != peer_id);
                    warn!("Removing peer {peer_id} after {} errors", error_count);

                    if self.available_peers.is_empty()
                        && self
                            .errors_per_peers
                            .iter()
                            .all(|(_, e)| *e >= self.max_errors_per_peer)
                    {
                        error!(
                            "No more peers available to ask for model blob tickets, terminate process"
                        );
                        cancellation_token.cancel();
                    }
                } else if !self.available_peers.contains(&peer_id) {
                    self.available_peers.push_back(peer_id);
                };
            }
        }
    }
}

async fn peer_manager_actor(
    mut rx: mpsc::UnboundedReceiver<PeerCommand>,
    max_errors_per_peer: u8,
    cancellation_token: CancellationToken,
) {
    let mut actor = PeerManagerActor::new(max_errors_per_peer);

    while let Some(message) = rx.recv().await {
        actor.handle_message(message, cancellation_token.clone());
    }
}

pub const ALPN: &[u8] = b"model-sharing/0";
pub const MODEL_REQUEST_TIMEOUT_SECS: u64 = 10;

#[derive(Error, Debug, serde::Serialize, serde::Deserialize)]
pub enum SharableModelError {
    #[error("Torch serialize error: {0}")]
    TchSerializeError(String),
    #[error("The update of the sharable model parameters is invalid")]
    InvalidUpdate,
    #[error("Parameter with name {0} is unknown")]
    ParameterUnknown(String),
    #[error("The parameter was already added")]
    ParameterAlreadyAdded,
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Parameters were not initialized")]
    ParametersNotInitialized,
    #[error("Parameter {0} is known but was not yet initialized")]
    ParameterNotInitialized(String),
    #[error("Response channel was not initialized")]
    ResponseChannelNotInitialized,
    #[error("Connection IO error: {0}")]
    ConnectionIOError(String),
    #[error("Could not decode UTF-8 string of model parameter name: {0}")]
    DecodeParameterNameError(String),
    #[error("Model config not initialized")]
    ModelConfigNotInitialized,
    #[error("Tokenizer config not initialized")]
    TokenizerConfigNotInitialized,
    #[error("Error parsing string to config: {0}")]
    ParseConfig(String),
    #[error("Could not send the config to the client")]
    SendConfig,
    #[error("Sharable parameter load thread crashed")]
    LoadThreadCrashed,
    #[error("P2P add download error: {0}")]
    P2PAddDownloadError(String),
    #[error("We don't have the model name and hash")]
    NameAndHashNotLoaded,
    #[error("P2P get own NodeAddr error: {0}")]
    P2PGetNodeAddrError(String),
    #[error("Requested model name: {0} does not match own model name: {1}")]
    MismatchedModelNameError(String, String),
    #[error("Error deserializing model: {0}")]
    DeserializeModelError(String),
    #[error("Model already initialized")]
    ModelAlreadyInitialized,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum ModelMetadataNetworkMessage {
    Ticket(Result<BlobTicket, SharableModelError>),
    ModelInfo(Result<(NodeAddr, Hash), SharableModelError>),
}

// This convertions are done manually since the original errors does not implement serialize and deserialize
impl From<tch::TchError> for SharableModelError {
    fn from(err: tch::TchError) -> Self {
        SharableModelError::TchSerializeError(err.to_string())
    }
}

impl From<std::io::Error> for SharableModelError {
    fn from(err: std::io::Error) -> Self {
        SharableModelError::ConnectionIOError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for SharableModelError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        SharableModelError::DecodeParameterNameError(err.to_string())
    }
}

impl From<serde_json::Error> for SharableModelError {
    fn from(err: serde_json::Error) -> Self {
        SharableModelError::ParseConfig(err.to_string())
    }
}

/// Represent the different types of requests that a new client can make to obtain the model.
/// It should request the Config first and extract the parameters from there.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ModelRequestType {
    /// Request for the model and tokenizer configs
    Config,
    /// Parameter request containing the parameter name
    Parameter(String),
    // temporary, we eventually want to move to just using this instead of Parameter
    ModelHash(String),
}

pub enum ParameterSharingMessage {
    Get(
        String,
        oneshot::Sender<Result<BlobTicket, SharableModelError>>,
    ),
}

pub enum ModelInfoSharingMessage {
    Get(oneshot::Sender<Result<(NodeAddr, Hash), SharableModelError>>),
}

pub enum ModelConfigSharingMessage {
    Get(oneshot::Sender<Result<BlobTicket, SharableModelError>>),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TransmittableModelParameter {
    param_name_bytes: Vec<u8>,
    param_value_bytes: Vec<u8>,
}

impl TransmittableModelParameter {
    fn new(param_name_bytes: Vec<u8>, param_value_bytes: Vec<u8>) -> Self {
        Self {
            param_name_bytes,
            param_value_bytes,
        }
    }

    pub fn name(&self) -> Result<String, SharableModelError> {
        Ok(String::from_utf8(self.param_name_bytes.clone())?)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TransmittableModelConfig {
    pub config: String,
    pub tokenizer: String,
}

impl TransmittableModelConfig {
    pub fn new(config: String, tokenizer: String) -> Self {
        Self { config, tokenizer }
    }
}

/// This data structure is the one responsible of storing the model config
/// and parameters for sharing them to other peers via p2p, as well as
/// storing them while parameters are downloaded from other peers.
#[derive(Debug)]
pub struct SharableModel {
    // use a BTreeMap so different systems have the elements in the same order
    // so we can (hopefully) mix and match serialized bytes from them
    //todo: maybe have a Cow<String> for the names, we're cloning them everwhere
    parameters: Option<BTreeMap<String, Option<TensorWrapper>>>,
    serializing_parameters: Option<
        HashMap<String, JoinHandle<Result<TransmittableModelParameter, SharableModelError>>>,
    >,
    serialized_parameters: Option<HashMap<String, BlobTicket>>,
    model_config: Option<String>,
    tokenizer_config: Option<Tokenizer>,
    config_and_tokenizer_ticket: Option<BlobTicket>,
    pub tx_model_config_response: Option<oneshot::Sender<(String, Tokenizer)>>,
    tx_params_response: Option<oneshot::Sender<HashMap<String, Tensor>>>,
    model_name: Option<String>,
    serialized_model: Option<BlobTicket>,
}
#[derive(Debug)]
struct TensorWrapper(Tensor);

impl Serialize for TensorWrapper {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bytes = Vec::new();
        if let Err(err) = self.save_to_stream(&mut bytes) {
            return Err(ser::Error::custom(err.to_string()));
        }

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for TensorWrapper {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        let buf_reader = Cursor::new(bytes);
        match Tensor::load_from_stream(buf_reader) {
            Err(err) => Err(de::Error::custom(err.to_string())),
            Ok(tensor) => Ok(Self(tensor)),
        }
    }
}

impl Deref for TensorWrapper {
    type Target = Tensor;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TensorWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// These impls are methods called by both the sharing model peers and the ones
// that download
impl SharableModel {
    pub fn empty() -> Self {
        Self {
            parameters: None,
            serializing_parameters: None,
            serialized_parameters: None,
            tx_params_response: None,
            model_config: None,
            tokenizer_config: None,
            config_and_tokenizer_ticket: None,
            tx_model_config_response: None,
            model_name: None,
            serialized_model: None,
        }
    }
}

// These impls on the `SharableModel` struct are the ones called by the
// peers that are in charge of sharing the parameters to the newly joined ones.
impl SharableModel {
    pub fn update_parameters(
        &mut self,
        new_parameters: HashMap<String, Tensor>,
    ) -> Result<(), SharableModelError> {
        debug!(
            "Updating sharable parameters with new {} new parameters",
            new_parameters.len()
        );

        if let Some(parameters) = &mut self.parameters {
            // validate that both models have the same parameters
            let new_parameters_names: HashSet<_> = new_parameters.keys().cloned().collect();
            let parameters_names: HashSet<_> = parameters.keys().cloned().collect();
            if new_parameters_names != parameters_names {
                return Err(SharableModelError::InvalidUpdate);
            }
        };

        let mut parameters = BTreeMap::new();
        let new_parameters = new_parameters;
        for (param_name, tensor) in &new_parameters {
            parameters.insert(
                param_name.clone(),
                Some(TensorWrapper(tensor.shallow_clone())),
            );
        }
        self.parameters = Some(parameters);

        let mut serialzing_parameters = HashMap::new();
        for (param_name, parameter) in new_parameters {
            serialzing_parameters.insert(
                param_name.clone(),
                tokio::task::spawn_blocking(move || {
                    let mut param_name_buffer = Vec::new();
                    let mut param_value_buffer = Vec::new();

                    param_name_buffer.write_all(param_name.as_bytes())?;
                    parameter.save_to_stream(&mut param_value_buffer)?;

                    let transmittable_parameter =
                        TransmittableModelParameter::new(param_name_buffer, param_value_buffer);

                    trace!("Finished serializing parameter {param_name} for sharing");
                    Ok(transmittable_parameter)
                }),
            );
        }
        self.serialized_parameters = Some(HashMap::new());
        self.serializing_parameters = Some(serialzing_parameters);
        Ok(())
    }

    pub fn update_config(
        &mut self,
        model_config: String,
        tokenizer_config: Tokenizer,
    ) -> Result<(), SharableModelError> {
        self.model_config = Some(model_config);
        self.tokenizer_config = Some(tokenizer_config);
        self.config_and_tokenizer_ticket = None;
        Ok(())
    }

    //todo: figure out a way of offloading this to another thread T_T
    pub async fn get_serialized_model<B: Networkable>(
        &mut self,
        p2p: &mut NetworkConnection<B, TransmittableDownload>,
    ) -> Result<BlobTicket, SharableModelError> {
        if let Some(ticket) = &self.serialized_model {
            return Ok(ticket.clone());
        }
        if let Some(map) = &self.parameters {
            //todo: check if every item is Some and return some kind of nice error, or better yet, just
            // get rid of the Option entirely
            let map_copy = map
                .iter()
                .map(|item| {
                    (
                        item.0.clone(),
                        Some(TensorWrapper(item.1.as_ref().unwrap().shallow_clone())),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let handle = tokio::task::spawn_blocking(move || postcard::to_allocvec(&map_copy));

            let bytes = handle
                .await
                .map_err(|_err| SharableModelError::LoadThreadCrashed)?
                .map_err(|err| SharableModelError::SerializationError(err.to_string()))?;

            let ticket = p2p
                .add_downloadable_raw(bytes, 0)
                .await
                .map_err(|err| SharableModelError::P2PAddDownloadError(err.to_string()))?;

            self.serialized_model = Some(ticket.clone());
            return Ok(ticket);
        }
        Err(SharableModelError::ParametersNotInitialized)
    }

    pub async fn get_transmittable_parameter<B: Networkable>(
        &mut self,
        param_name: &str,
        p2p: &mut NetworkConnection<B, TransmittableDownload>,
        tag: u32,
    ) -> Result<BlobTicket, SharableModelError> {
        let Some(loading_parameters) = self.serializing_parameters.as_mut() else {
            return Err(SharableModelError::ParametersNotInitialized);
        };
        let Some(loaded_parameters) = self.serialized_parameters.as_mut() else {
            return Err(SharableModelError::ParametersNotInitialized);
        };

        match loaded_parameters.get(param_name) {
            Some(blob_ticket) => {
                info!("Using cached downloadable for {param_name}");
                Ok(blob_ticket.clone())
            }
            None => match loading_parameters.remove(param_name) {
                Some(loading) => {
                    trace!("Waiting for {param_name} parameter to finish serializing");
                    let transmittable_parameter = loading
                        .await
                        .map_err(|_| SharableModelError::LoadThreadCrashed)??;
                    let transmittable_download =
                        TransmittableDownload::ModelParameter(transmittable_parameter);
                    trace!("Adding parameter downloadable {param_name}");
                    let blob_ticket = p2p
                        .add_downloadable(transmittable_download, tag)
                        .await
                        .map_err(|err| SharableModelError::P2PAddDownloadError(err.to_string()))?;
                    loaded_parameters.insert(param_name.to_string(), blob_ticket.clone());
                    info!("Finished adding parameter downloadable {param_name}");
                    Ok(blob_ticket)
                }
                None => Err(SharableModelError::ParameterUnknown(param_name.to_string())),
            },
        }
    }

    //todo: i have no idea why we need to borrow as mut here, but if we don't rust complains that SharableModel needs to be Send T_T
    pub async fn get_model_name_and_hash<B: Networkable>(
        &mut self,
        p2p: &mut NetworkConnection<B, TransmittableDownload>,
    ) -> Result<(NodeAddr, Hash), SharableModelError> {
        if let Some(ticket) = self.serialized_model.as_ref() {
            /*if desired_name != name {
                return Err(SharableModelError::MismatchedModelNameError(desired_name, name));
            }*/

            Ok((ticket.node_addr().clone(), ticket.hash()))
        } else if let Some(_) = self.parameters.as_ref() {
            self.get_serialized_model(p2p).await?;
            let ticket = self.serialized_model.as_ref().unwrap();
            Ok((ticket.node_addr().clone(), ticket.hash()))
        } else {
            Err(SharableModelError::NameAndHashNotLoaded)
        }
    }

    /// Used for clients that already have the config and needs to share it via p2p.
    pub async fn get_transmittable_config<B: Networkable>(
        &mut self,
        p2p: &mut NetworkConnection<B, TransmittableDownload>,
        tag: u32,
    ) -> Result<BlobTicket, SharableModelError> {
        match self.config_and_tokenizer_ticket.as_ref() {
            Some(ticket) => {
                trace!("Using cached config and tokenizer downloadable");
                Ok(ticket.clone())
            }
            None => {
                trace!("Building config and tokenizer downloadable");
                let Some(config) = self.model_config.as_ref() else {
                    return Err(SharableModelError::ModelConfigNotInitialized);
                };
                let Some(tokenizer) = self.tokenizer_config.as_ref() else {
                    return Err(SharableModelError::TokenizerConfigNotInitialized);
                };
                let raw_tokenizer = tokenizer
                    .to_string(false)
                    .map_err(|err| SharableModelError::ParseConfig(err.to_string()))?;
                let transmittable_config: TransmittableModelConfig =
                    TransmittableModelConfig::new(config.clone(), raw_tokenizer);
                let transmittable_download =
                    TransmittableDownload::ModelConfig(transmittable_config);
                let ticket = p2p
                    .add_downloadable(transmittable_download, tag)
                    .await
                    .map_err(|err| SharableModelError::P2PAddDownloadError(err.to_string()))?;
                self.config_and_tokenizer_ticket = Some(ticket.clone());
                Ok(ticket)
            }
        }
    }

    pub fn clear_cache(&mut self) {
        self.config_and_tokenizer_ticket = None;
        self.serialized_parameters = None;
    }
}

// These impls on the `SharableModel` struct are the ones called by the
// new peers that are joining a run and have to download parameters from peers
// that are sharing them.
impl SharableModel {
    // Initialize the model parameter names. This is important to know when
    // all model parameters have been downloaded from other peers.
    pub fn initialize_parameters(
        &mut self,
        param_names: &[String],
        model_name: String,
        tx_params_response: oneshot::Sender<HashMap<String, Tensor>>,
    ) {
        // Initialize the model parameter names with None.
        let mut parameters = BTreeMap::new();
        for param_name in param_names {
            parameters.insert(param_name.clone(), None);
        }
        self.parameters = Some(parameters);
        self.tx_params_response = Some(tx_params_response);
        self.model_name = Some(model_name);
    }

    pub async fn deserialize_params(&mut self, data: Vec<u8>) -> Result<(), SharableModelError> {
        if let Some(_) = &self.parameters {
            //return Err(SharableModelError::ModelAlreadyInitialized);
        }

        let handle = tokio::task::spawn_blocking(move || {
            postcard::from_bytes::<BTreeMap<String, Option<TensorWrapper>>>(&data)
        });

        let params = handle
            .await
            .map_err(|_err| SharableModelError::LoadThreadCrashed)?
            .map_err(|err| SharableModelError::DeserializeModelError(err.to_string()))?;
        self.parameters = Some(params);

        Ok(())
    }

    // Add new parameter downloaded from another peer
    pub async fn add_parameter(
        &mut self,
        parameter: TransmittableModelParameter,
    ) -> Result<(), SharableModelError> {
        let Some(parameters) = self.parameters.as_mut() else {
            return Err(SharableModelError::ParametersNotInitialized);
        };

        // Deserialize model parameter
        let param_name = parameter.name()?;
        let buf_reader = Cursor::new(parameter.param_value_bytes);
        trace!("Start loading parameter {param_name}");
        let param_value = tokio::task::spawn_blocking(move || Tensor::load_from_stream(buf_reader))
            .await
            .map_err(|_| SharableModelError::LoadThreadCrashed)??;
        trace!("Finished loading parameter {param_name}");

        // Validate that the parameter does not already exist
        // This should be called only by a client that joins the run
        match parameters.entry(param_name.to_string()) {
            Entry::Occupied(mut param_entry) => {
                let param = param_entry.get_mut();
                if param.is_some() {
                    warn!(
                        "Parameter {} was already added to the model, ignoring it",
                        param_name
                    );
                }
                *param = Some(TensorWrapper(param_value));
                Ok(())
            }
            Entry::Vacant(_) => Err(SharableModelError::ParameterUnknown(param_name.to_string())),
        }
    }

    /// Add the config downloaded from other peer
    pub fn add_config(
        &mut self,
        transmittable_config: TransmittableModelConfig,
    ) -> Result<(), SharableModelError> {
        let config = transmittable_config.config;
        let tokenizer: Tokenizer = serde_json::from_str(&transmittable_config.tokenizer)?;

        self.model_config = Some(config);
        self.tokenizer_config = Some(tokenizer);
        Ok(())
    }

    // Utility function that is used to know when we have downloaded all
    // model parameters from the other peers
    pub fn is_download_complete(&self) -> bool {
        let Some(parameters) = self.parameters.as_ref() else {
            return false;
        };

        parameters
            .iter()
            .all(|(_param_name, param_value)| param_value.is_some())
    }

    // Once all parameters have been downloaded, this function is called to send them
    // to the initialization task, so that the model can be loaded
    pub fn send_init_parameters(&mut self) -> Result<(), SharableModelError> {
        if let Some(tx_params_response) = self.tx_params_response.take() {
            let Some(parameters) = self.parameters.take() else {
                return Err(SharableModelError::ParametersNotInitialized);
            };

            let mut parameters_to_send = HashMap::new();
            for (param_name, parameter) in parameters.into_iter() {
                let Some(tensor) = parameter else {
                    // This error should never really happen, but checking just in case
                    // something goes really wrong
                    return Err(SharableModelError::ParameterNotInitialized(param_name));
                };
                parameters_to_send.insert(param_name, tensor.0);
            }
            tx_params_response.send(parameters_to_send).unwrap();
            return Ok(());
        }
        Err(SharableModelError::ResponseChannelNotInitialized)
    }

    /// Send the model config back to the initial run task for the client to create the model.
    pub fn send_config(&mut self) -> Result<(), SharableModelError> {
        if let Some(tx_model_config_response) = self.tx_model_config_response.take() {
            let Some(config) = self.model_config.clone() else {
                return Err(SharableModelError::ModelConfigNotInitialized);
            };
            let Some(tokenizer) = self.tokenizer_config.clone() else {
                return Err(SharableModelError::TokenizerConfigNotInitialized);
            };
            tx_model_config_response
                .send((config, tokenizer))
                .map_err(|_e| SharableModelError::SendConfig)?;
            return Ok(());
        }
        Err(SharableModelError::ResponseChannelNotInitialized)
    }
}

#[derive(Debug, Clone)]
pub struct ModelSharing {
    tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>,
    tx_model_config_req: UnboundedSender<ModelConfigSharingMessage>,
    tx_model_info_req: UnboundedSender<ModelInfoSharingMessage>,
}

impl ModelSharing {
    pub fn new(
        tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>,
        tx_model_config_req: UnboundedSender<ModelConfigSharingMessage>,
        tx_model_info_req: UnboundedSender<ModelInfoSharingMessage>,
    ) -> Self {
        Self {
            tx_model_parameter_req,
            tx_model_config_req,
            tx_model_info_req,
        }
    }
    pub(crate) fn _accept_connection(
        connection: Connection,
        tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>,
        tx_model_config_req: UnboundedSender<ModelConfigSharingMessage>,
        tx_model_info_req: UnboundedSender<ModelInfoSharingMessage>,
    ) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            let (mut send, mut recv) = connection.accept_bi().await?;
            let model_request_type_bytes = recv.read_to_end(1000).await?;
            let model_request_type = ModelRequestType::from_bytes(&model_request_type_bytes)?;
            // todo: this is so bad, fix this T_T
            // we should be

            let response = match model_request_type {
                ModelRequestType::Parameter(parameter_request) => {
                    // Create channel for requesting the model parameter to the client backend
                    // and add a new blob for it
                    let (tx_req, rx_req) =
                        oneshot::channel::<Result<BlobTicket, SharableModelError>>();
                    let request = ParameterSharingMessage::Get(parameter_request, tx_req);
                    tx_model_parameter_req.send(request)?;

                    // Receive the blob ticket and forward it to the requesting client
                    ModelMetadataNetworkMessage::Ticket(rx_req.await?)
                }
                ModelRequestType::ModelHash(name) => {
                    let (tx_req, rx_req) = oneshot::channel();
                    let request = ModelInfoSharingMessage::Get(tx_req);
                    tx_model_info_req.send(request)?;

                    // Receive the blob ticket and forward it to the requesting client
                    ModelMetadataNetworkMessage::ModelInfo(rx_req.await?)
                }
                ModelRequestType::Config => {
                    // Create channel for requesting the model config to the client backend and add a new blob for it
                    let (tx_req, rx_req) =
                        oneshot::channel::<Result<BlobTicket, SharableModelError>>();
                    let request = ModelConfigSharingMessage::Get(tx_req);
                    tx_model_config_req.send(request)?;

                    // Receive the blob ticket and forward it to the requesting client
                    ModelMetadataNetworkMessage::Ticket(rx_req.await?)
                }
            };

            match response {
                ModelMetadataNetworkMessage::Ticket(blob_ticket) => {
                    let data = postcard::to_stdvec(&blob_ticket)?;
                    send.write_all(&data).await?;
                    send.finish()?;
                }
                ModelMetadataNetworkMessage::ModelInfo(info) => {
                    let data = postcard::to_stdvec(&info)?;
                    send.write_all(&data).await?;
                    send.finish()?;
                }
            }

            // Wait until the remote closes the connection, which it does once it
            // received the response.
            connection.closed().await;

            Ok(())
        })
    }

    pub fn accept_connection(&self, connection: Connection) -> BoxedFuture<Result<()>> {
        let tx_model_parameter_req = self.tx_model_parameter_req.clone();
        let tx_model_config_req = self.tx_model_config_req.clone();
        let tx_model_info_req = self.tx_model_info_req.clone();
        Box::pin(async move {
            Self::_accept_connection(
                connection,
                tx_model_parameter_req,
                tx_model_config_req,
                tx_model_info_req,
            )
            .await
        })
    }
}

impl ProtocolHandler for ModelSharing {
    //todo: why not just call Self::accept_connection ?
    fn accept(&self, connection: Connection) -> BoxedFuture<Result<()>> {
        let tx_model_parameter_req = self.tx_model_parameter_req.clone();
        let tx_model_config_req = self.tx_model_config_req.clone();
        let tx_model_info_req = self.tx_model_info_req.clone();
        Box::pin(async move {
            Self::_accept_connection(
                connection,
                tx_model_parameter_req,
                tx_model_config_req,
                tx_model_info_req,
            )
            .await
        })
    }
}
