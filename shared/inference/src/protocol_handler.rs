//! Direct P2P protocol handler for inference requests
//!
//! This implements iroh's ProtocolHandler trait to accept incoming
//! inference requests over direct P2P connections.

use crate::{InferenceMessage, InferenceNode, InferenceRequest, InferenceResponse};
use anyhow::{Context, Result};
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub const INFERENCE_ALPN: &[u8] = b"/psyche/inference/1";

#[derive(Clone, Debug)]
pub struct InferenceProtocol {
    inference_node: Arc<RwLock<Option<InferenceNode>>>,
}

impl InferenceProtocol {
    pub fn new(inference_node: Arc<RwLock<Option<InferenceNode>>>) -> Self {
        Self { inference_node }
    }

    fn create_error_response(request_id: String, error: &str) -> InferenceResponse {
        InferenceResponse {
            request_id,
            generated_text: String::new(),
            full_text: String::new(),
            finish_reason: Some(format!("error: {}", error)),
        }
    }

    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        let peer_id = connection.remote_id();
        debug!(
            "Accepting inference connection from {}",
            peer_id.fmt_short()
        );

        // bidirectional stream
        let (mut send, mut recv) = connection.accept_bi().await?;

        let request_bytes = recv.read_to_end(1024 * 1024).await?;
        let message: InferenceMessage = postcard::from_bytes(&request_bytes)
            .context("Failed to deserialize inference message")?;

        match message {
            InferenceMessage::Request(request) => {
                info!(
                    "Received inference request {} from {} (streaming: {})",
                    request.request_id,
                    peer_id.fmt_short(),
                    request.stream
                );

                if request.stream {
                    // Streaming mode: send chunks as they arrive
                    self.handle_streaming_request(request, send).await?;
                } else {
                    // Non-streaming mode: send single response
                    let response = self.process_request(request).await?;

                    info!("Serializing response for {}", peer_id.fmt_short());
                    let response_msg = InferenceMessage::Response(response);
                    let response_bytes = postcard::to_allocvec(&response_msg)
                        .context("Failed to serialize response")?;

                    info!(
                        "Writing {} bytes to {}",
                        response_bytes.len(),
                        peer_id.fmt_short()
                    );
                    send.write_all(&response_bytes).await?;

                    info!("Finishing send stream to {}", peer_id.fmt_short());
                    send.finish()?;

                    // adaptive delay to ensure data is flushed before connection is dropped
                    // without this, the connection might close before the peer reads all bytes
                    // base 50ms + 10ms per MB of data
                    let size_mb = response_bytes.len() as f64 / (1024.0 * 1024.0);
                    let delay_ms = 50 + (size_mb * 10.0) as u64;
                    debug!(
                        "Waiting {}ms for {} bytes to flush",
                        delay_ms,
                        response_bytes.len()
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

                    info!(
                        "Successfully sent inference response to {}",
                        peer_id.fmt_short()
                    );
                }
            }
            _ => {
                error!("Unexpected message type from {}", peer_id.fmt_short());
            }
        }

        Ok(())
    }

    async fn process_request(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let node = self.inference_node.read().await;

        match node.as_ref() {
            Some(node) => {
                info!("Processing inference request: {}", request.request_id);
                node.inference(&request).context("Failed to run inference")
            }
            None => {
                error!("Inference node not initialized");
                Ok(Self::create_error_response(
                    request.request_id,
                    "node not initialized",
                ))
            }
        }
    }

    async fn handle_streaming_request(
        &self,
        request: InferenceRequest,
        mut send: iroh::endpoint::SendStream,
    ) -> Result<()> {
        use crate::vllm::StreamChunk;

        let node = self.inference_node.read().await;

        match node.as_ref() {
            Some(node) => {
                info!(
                    "Processing streaming inference request: {}",
                    request.request_id
                );

                // Create a channel to send chunks from the callback to the async stream writer
                let (chunk_tx, mut chunk_rx) =
                    tokio::sync::mpsc::unbounded_channel::<StreamChunk>();

                let request_id = request.request_id.clone();

                // Spawn a blocking task to run the inference (Python GIL blocking)
                let node_clone = node.clone();
                let handle = tokio::task::spawn_blocking(move || {
                    node_clone.inference_streaming(&request, |chunk: StreamChunk| {
                        // Send chunk through channel
                        let _ = chunk_tx.send(chunk);
                    })
                });

                // Stream chunks as they arrive
                while let Some(chunk) = chunk_rx.recv().await {
                    let chunk_msg = InferenceMessage::StreamChunk {
                        request_id: request_id.clone(),
                        text: chunk.text,
                    };

                    let chunk_bytes = postcard::to_allocvec(&chunk_msg)
                        .context("Failed to serialize stream chunk")?;

                    debug!(
                        "Sending chunk ({} bytes) for request {}",
                        chunk_bytes.len(),
                        request_id
                    );

                    send.write_all(&chunk_bytes).await?;
                }

                // Wait for inference to complete
                let response = handle.await.context("Inference task panicked")??;

                // Send final response
                info!(
                    "Sending final response for streaming request {}",
                    request_id
                );
                let response_msg = InferenceMessage::Response(response);
                let response_bytes =
                    postcard::to_allocvec(&response_msg).context("Failed to serialize response")?;

                send.write_all(&response_bytes).await?;
                send.finish()?;

                // Adaptive delay
                let size_mb = response_bytes.len() as f64 / (1024.0 * 1024.0);
                let delay_ms = 50 + (size_mb * 10.0) as u64;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

                info!("Successfully completed streaming request {}", request_id);
                Ok(())
            }
            None => {
                error!("Inference node not initialized");
                let error_response =
                    Self::create_error_response(request.request_id, "node not initialized");

                let response_msg = InferenceMessage::Response(error_response);
                let response_bytes =
                    postcard::to_allocvec(&response_msg).context("Failed to serialize response")?;

                send.write_all(&response_bytes).await?;
                send.finish()?;

                Ok(())
            }
        }
    }
}

impl ProtocolHandler for InferenceProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.handle_connection(connection).await.map_err(|e| {
            error!("Error handling inference connection: {:#}", e);
            let io_error = std::io::Error::other(e.to_string());
            AcceptError::from_err(io_error)
        })
    }
}
