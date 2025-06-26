use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::{net_protocol::Blobs, store::mem::Store};
use iroh_gossip::net::Gossip;
use tokio::{sync::Mutex, task::JoinSet};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use tracing::{Instrument, error, info_span, trace, warn};

use iroh::{Endpoint, protocol::ProtocolHandler};

use crate::{Allowlist, ModelSharing, p2p_model_sharing};

/// TODO: This entire struct can be replaced with the builtin Router using the new connection
/// limiting functionality in Iroh:
/// https://github.com/n0-computer/iroh/pull/3157/commits/54e6b66d4292ad0b38ac479b13c3a96776d23d08
///
/// The allowlist-enabled router.
/// This is mostly verbatim from Iroh's source, just modified to let us insert the allowlist.
///
/// Construct this using [`Router::spawn`].
///
/// When dropped, this will abort listening the tasks, so make sure to store it.
///
/// Even with this abort-on-drop behaviour, it's recommended to call and await
/// [`Router::shutdown`] before ending the process.
///
/// As an example for graceful shutdown, e.g. for tests or CLI tools,
/// wait for [`tokio::signal::ctrl_c()`]
#[derive(Clone, Debug)]
pub struct Router {
    endpoint: Endpoint,
    // `Router` needs to be `Clone + Send`, and we need to `task.await` in its `shutdown()` impl.
    task: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    cancel_token: CancellationToken,
}

impl Router {
    /// Returns the [`Endpoint`] stored in this router.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Checks if the router is already shutdown.
    pub fn is_shutdown(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Shuts down the accept loop cleanly.
    ///
    /// When this function returns, all [`ProtocolHandler`]s will be shutdown and
    /// `Endpoint::close` will have been called.
    ///
    /// If already shutdown, it returns `Ok`.
    ///
    /// If some [`ProtocolHandler`] panicked in the accept loop, this will propagate
    /// that panic into the result here.
    pub async fn shutdown(&self) -> Result<()> {
        if self.is_shutdown() {
            return Ok(());
        }

        // Trigger shutdown of the main run task by activating the cancel token.
        self.cancel_token.cancel();

        // Wait for the main task to terminate.
        if let Some(task) = self.task.lock().await.take() {
            task.await?;
        }

        Ok(())
    }

    /// Spawns a new Router using given [`Endpoint`], allowlist, and protocol impls.
    pub async fn spawn<A: Allowlist + 'static + Send>(
        endpoint: Endpoint,
        gossip: Gossip,
        blobs: Blobs<Store>,
        p2p_model_sharing: ModelSharing,
        allowlist: A,
    ) -> Result<Self> {
        endpoint.set_alpns(vec![
            iroh_blobs::ALPN.to_vec(),
            iroh_gossip::ALPN.to_vec(),
            p2p_model_sharing::ALPN.to_vec(),
        ]);

        let cancel = CancellationToken::new();
        let cancel_token = cancel.clone();

        let run_loop_fut = {
            let mut join_set = JoinSet::new();
            let endpoint = endpoint.clone();
            let gossip = gossip.clone();
            let blobs = blobs.clone();
            let p2p_model_sharing = p2p_model_sharing.clone();
            let allowlist = Box::new(allowlist);

            async move {
                // Make sure to cancel the token, if this future ever exits.
                let _cancel_guard = cancel_token.clone().drop_guard();

                loop {
                    tokio::select! {
                        biased;
                        _ = cancel_token.cancelled() => {
                            break;
                        },
                        // handle task terminations and quit on panics.
                        Some(res) = join_set.join_next() => {
                            match res {
                                Err(outer) => {
                                    if outer.is_panic() {
                                        error!("Task panicked: {outer:?}");
                                        break;
                                    } else if outer.is_cancelled() {
                                        trace!("Task cancelled: {outer:?}");
                                    } else {
                                        error!("Task failed: {outer:?}");
                                        break;
                                    }
                                }
                                Ok(Some(())) => {
                                    trace!("Task finished");
                                }
                                Ok(None) => {
                                    trace!("Task cancelled");
                                }
                            }
                        },

                        // handle incoming p2p connections.
                        incoming = endpoint.accept() => {
                            let Some(incoming) = incoming else {
                                break; // endpoint is now closed, exit accept loop.
                            };

                            let token = cancel_token.child_token();

                            let gossip = gossip.clone();
                            let blobs = blobs.clone();
                            let allowlist = allowlist.clone();
                            let p2p_model_sharing = p2p_model_sharing.clone();
                            join_set.spawn(async move {
                                token.run_until_cancelled(handle_connection(incoming, gossip, blobs, p2p_model_sharing, allowlist)).await
                            }.instrument(info_span!("router.accept")));
                        },
                    }
                }

                shutdown(&endpoint, gossip, blobs, p2p_model_sharing).await;

                // Abort remaining tasks.
                tracing::info!("Shutting down remaining tasks");
                join_set.shutdown().await;
            }
        };
        let task = tokio::task::spawn(run_loop_fut);
        let task = AbortOnDropHandle::new(task);

        Ok(Router {
            endpoint,
            task: Arc::new(Mutex::new(Some(task))),
            cancel_token: cancel,
        })
    }
}

/// Shutdown the different parts of the router concurrently.
async fn shutdown(
    endpoint: &Endpoint,
    gossip: Gossip,
    blobs: Blobs<Store>,
    p2p_model_sharing: ModelSharing,
) {
    // We ignore all errors during shutdown.
    let _ = tokio::join!(
        // Close the endpoint.
        endpoint.close(),
        // Shutdown protocol handlers, using the ProtocolHandler shutdown impl.
        (&gossip as &dyn ProtocolHandler).shutdown(),
        (&blobs as &dyn ProtocolHandler).shutdown(),
        (&p2p_model_sharing as &dyn ProtocolHandler).shutdown(),
    );
}

async fn handle_connection<A: Allowlist + 'static + Send>(
    incoming: iroh::endpoint::Incoming,
    gossip: Gossip,
    blobs: Blobs<Store>,
    p2p_model_sharing: ModelSharing,
    allowlist: Box<A>,
) {
    let mut connecting = match incoming.accept() {
        Ok(conn) => conn,
        Err(err) => {
            warn!("Ignoring connection: accepting failed: {err:#}");
            return;
        }
    };

    let alpn = match connecting.alpn().await {
        Ok(alpn) => alpn,
        Err(err) => {
            warn!("Ignoring connection: invalid handshake: {err:#}");
            return;
        }
    };

    let connection = match connecting.await {
        Ok(connection) => connection,
        Err(err) => {
            warn!("Failed to establish connection: {err:#}");
            return;
        }
    };

    let node_id = match connection.remote_node_id() {
        Ok(node_id) => node_id,
        Err(err) => {
            connection.close(0u8.into(), b"no node id given");
            warn!("Failed to get node id while connecting: {err:#}");
            return;
        }
    };

    if !allowlist.allowed(node_id) {
        // kill connection completely!
        connection.close(0u8.into(), b"not in allowlist");
        warn!("Killing attemption connection: Node ID {node_id} is not in allowlist.");
        return;
    }

    if alpn == iroh_gossip::ALPN {
        if let Err(err) = gossip.handle_connection(connection).await {
            warn!("Handling incoming gossip connection ended with error: {err}");
        };
    } else if alpn == iroh_blobs::ALPN {
        let db = blobs.store().clone();
        let events = blobs.events().clone();
        let rt = blobs.rt().clone();
        iroh_blobs::provider::handle_connection(connection, db, events, rt).await;
    } else if alpn == p2p_model_sharing::ALPN {
        if let Err(err) = p2p_model_sharing.accept_connection(connection).await {
            warn!("Handling incoming p2p model sharing connection ended with error: {err}")
        }
    } else {
        warn!("Ignoring connection: unsupported ALPN protocol");
        return;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::future::join_all;
    use iroh::SecretKey;
    use iroh_gossip::{
        net::{Event, GossipEvent, Message},
        proto::TopicId,
    };
    use tokio_stream::StreamExt;

    use crate::{
        ModelSharing,
        allowlist::{AllowAll, AllowDynamic},
    };

    use super::*;

    #[tokio::test]
    async fn test_shutdown() -> Result<()> {
        let endpoint = Endpoint::builder().bind().await?;
        let blobs = Blobs::memory().build(&endpoint);
        let gossip = Gossip::builder().spawn(endpoint.clone()).await?;
        let (tx_model_parameter_req, _rx_model_parameter_req) =
            tokio::sync::mpsc::unbounded_channel();
        let (tx_model_config_req, _rx_model_config_req) = tokio::sync::mpsc::unbounded_channel();
        let p2p_model_sharing = ModelSharing::new(tx_model_parameter_req, tx_model_config_req);

        let router = Router::spawn(
            endpoint.clone(),
            gossip.clone(),
            blobs.clone(),
            p2p_model_sharing.clone(),
            AllowAll,
        )
        .await?;

        assert!(!router.is_shutdown());
        assert!(!endpoint.is_closed());

        router.shutdown().await?;

        assert!(router.is_shutdown());
        assert!(endpoint.is_closed());

        Ok(())
    }

    /// Tests the allowlist functionality by:
    /// 1. Setting up N_CLIENTS routers where only N_ALLOWED are whitelisted
    /// 2. Having each client broadcast a message
    /// 3. Verifying that only messages from allowed clients are received
    #[tokio::test]
    async fn test_allowlist() -> Result<()> {
        const N_CLIENTS: u8 = 4;
        const N_ALLOWED: u8 = 3;

        // randomly initialized topic ID bytes.
        const GOSSIP_TOPIC: TopicId = TopicId::from_bytes([
            0x92, 0x41, 0xf9, 0xdd, 0xbd, 0x2d, 0xb1, 0xf0, 0xeb, 0xd0, 0xfd, 0xb1, 0xf5, 0x5a,
            0xaf, 0x73, 0xa5, 0xa0, 0x3b, 0x9e, 0xec, 0xe6, 0x92, 0x05, 0x9b, 0x45, 0x77, 0xe6,
            0x99, 0x45, 0x21, 62,
        ]);

        const _: () = assert!(N_ALLOWED < N_CLIENTS);

        let keys: Vec<SecretKey> = (0..N_CLIENTS)
            .map(|i| {
                SecretKey::from_bytes(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, i,
                ])
            })
            .collect();

        let pubkeys: Vec<_> = keys
            .iter()
            .take(N_ALLOWED as usize)
            .map(|k| k.public())
            .collect();

        // create a router for each key
        let routers = join_all(
            keys.into_iter()
                .map(|k| async {
                    let allowlist = AllowDynamic::with_nodes(pubkeys.clone());
                    let endpoint = Endpoint::builder().secret_key(k).bind().await?;
                    let blobs = Blobs::memory().build(&endpoint);
                    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;
                    let (tx_model_parameter_req, _rx_model_parameter_req) =
                        tokio::sync::mpsc::unbounded_channel();
                    let (tx_model_config_req, _rx_model_parameter_req) =
                        tokio::sync::mpsc::unbounded_channel();
                    let p2p_model_sharing =
                        ModelSharing::new(tx_model_parameter_req, tx_model_config_req);

                    Ok((
                        gossip.clone(),
                        Router::spawn(
                            endpoint.clone(),
                            gossip.clone(),
                            blobs.clone(),
                            p2p_model_sharing.clone(),
                            allowlist,
                        )
                        .await?,
                        endpoint.node_addr().await?,
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;

        let node_addrs: Vec<_> = routers.iter().map(|(_, _, node_addr)| node_addr).collect();

        // Set up gossip subscriptions for all routers
        let mut subscriptions = Vec::new();
        for (i, (gossip, router, _)) in routers.iter().enumerate() {
            for (j, a) in node_addrs.iter().enumerate() {
                if i != j {
                    router.endpoint().add_node_addr((*a).clone())?;
                }
            }
            let mut sub = gossip.subscribe(GOSSIP_TOPIC, pubkeys.clone())?;
            println!("subscribing {i} to topic..");

            subscriptions.push(async move {
                if i < N_ALLOWED as usize {
                    println!("waiting for {i} to get at least 1 peer..");
                    sub.joined().await.unwrap();
                    println!("gossip connections {i} ready");
                }
                let (gossip_tx, gossip_rx) = sub.split();
                (gossip_tx, gossip_rx)
            });
        }

        println!("waiting for gossip connections..");
        let mut subscriptions = join_all(subscriptions).await;
        println!("all gossip connections set up.");

        // Send messages from all clients
        for (i, (gossip_tx, _)) in subscriptions.iter_mut().enumerate() {
            let message = format!("Message from client {i}");
            println!("broadcasting {message}");
            gossip_tx.broadcast(message.into()).await?;
        }

        // Wait for messages to propagate
        println!("checking for recv'd messages..");

        // Check received messages
        for (i, (_, ref mut gossip_rx)) in subscriptions.iter_mut().enumerate() {
            let mut received_messages = Vec::new();
            while let Ok(Some(Ok(msg))) =
                tokio::time::timeout(Duration::from_millis(1000), gossip_rx.next()).await
            {
                if let Event::Gossip(GossipEvent::Received(Message { content, .. })) = msg {
                    let message = String::from_utf8(content.to_vec())?;

                    received_messages.push(message);
                } else if let Event::Lagged = msg {
                    panic!("lagged..");
                }
            }

            // Verify that messages from non-allowed clients (i > N_ALLOWED) are not received
            for message in &received_messages {
                let sender_id = message
                    .strip_prefix("Message from client ")
                    .and_then(|n| n.parse::<u8>().ok())
                    .expect("Invalid message format");

                assert!(
                    sender_id <= N_ALLOWED,
                    "Router {i} received message from non-allowed client {sender_id}"
                );
            }

            // Verify that all messages from allowed clients are received
            if i < N_ALLOWED as usize {
                assert_eq!(
                    received_messages.len(),
                    N_ALLOWED as usize - 1, // -1 because we're one of them!
                    "Router {i} didn't receive all allowed messages. only saw {received_messages:?}"
                );
            }
        }

        Ok(())
    }
}
