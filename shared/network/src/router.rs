use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::BlobsProtocol;
use iroh_gossip::net::Gossip;

use iroh::{
    Endpoint,
    protocol::{ProtocolHandler, Router},
};

use crate::{ModelSharing, p2p_model_sharing};

pub struct SupportedProtocols(Gossip, BlobsProtocol, ModelSharing);

impl SupportedProtocols {
    pub fn new(
        gossip: Gossip,
        blobs_protocol: BlobsProtocol,
        model_parameter_sharing: ModelSharing,
    ) -> Self {
        SupportedProtocols(gossip, blobs_protocol, model_parameter_sharing)
    }
}

pub(crate) fn spawn_router<P: ProtocolHandler + Clone>(
    endpoint: Endpoint,
    protocols: SupportedProtocols,
    additional_protocol: Option<(&'static [u8], P)>,
    iroh_n0des_host: Option<iroh_n0des::ClientHost>,
) -> Result<Arc<Router>> {
    let mut builder = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, protocols.0)
        .accept(iroh_blobs::ALPN, protocols.1)
        .accept(p2p_model_sharing::ALPN, protocols.2);

    // add optional custom protocol if provided
    if let Some((alpn, handler)) = additional_protocol {
        builder = builder.accept(alpn, handler);
    }

    if let Some(host) = iroh_n0des_host {
        builder = builder.accept(iroh_n0des::CLIENT_HOST_ALPN, host);
    }

    let router = Arc::new(builder.spawn());

    Ok(router)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::future::join_all;
    use iroh::{Endpoint, RelayMode, SecretKey, address_lookup::memory::MemoryLookup};
    use iroh_blobs::store::mem::MemStore;
    use iroh_gossip::{
        api::{Event, Message},
        net::Gossip,
        proto::TopicId,
    };
    use rand::Fill;
    use tokio_stream::StreamExt;

    use crate::allowlist::{AllowDynamic, AllowlistHook};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_shutdown() -> Result<()> {
        let endpoint = Endpoint::builder()
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await?;
        let blobs = MemStore::new();
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let (tx_model_parameter_req, _rx_model_parameter_req) =
            tokio::sync::mpsc::unbounded_channel();
        let (tx_model_config_req, _rx_model_config_req) = tokio::sync::mpsc::unbounded_channel();
        let p2p_model_sharing = ModelSharing::new(tx_model_parameter_req, tx_model_config_req);
        let blobs_protocol = BlobsProtocol::new(&blobs, None);
        let router = spawn_router::<iroh_gossip::net::Gossip>(
            endpoint.clone(),
            SupportedProtocols::new(gossip.clone(), blobs_protocol, p2p_model_sharing),
            None,
            None,
        )?;

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
    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_allowlist() -> Result<()> {
        const N_CLIENTS: u8 = 4;
        const N_ALLOWED: u8 = 3;

        // randomly initialized topic ID bytes.
        let gossip_topic: TopicId = TopicId::from_bytes({
            let mut bytes: [_; _] = [0; _];
            bytes.fill(&mut rand::rng());
            bytes
        });

        const _: () = assert!(N_ALLOWED < N_CLIENTS);

        let keys: Vec<SecretKey> = (0..N_CLIENTS)
            .map(|_| SecretKey::generate(&mut rand::rng()))
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
                    let static_discovery = MemoryLookup::new();
                    let endpoint = Endpoint::builder()
                        .secret_key(k)
                        .relay_mode(RelayMode::Disabled)
                        .clear_address_lookup()
                        .address_lookup(static_discovery.clone())
                        .hooks(AllowlistHook::new(allowlist))
                        .bind()
                        .await?;
                    let gossip = Gossip::builder().spawn(endpoint.clone());

                    let router = Arc::new(
                        Router::builder(endpoint.clone())
                            .accept(iroh_gossip::ALPN, gossip.clone())
                            .spawn(),
                    );

                    Ok((gossip.clone(), router, static_discovery))
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;

        // Set up gossip subscriptions for all routers
        let mut subscriptions = Vec::new();
        for (i, (gossip, router, static_discovery)) in routers.iter().enumerate() {
            for (_, router, _) in routers.iter() {
                static_discovery.add_endpoint_info(router.endpoint().addr());
            }
            let mut sub = gossip
                .subscribe(
                    gossip_topic,
                    pubkeys
                        .iter()
                        .filter(|k| **k != router.endpoint().id())
                        .cloned()
                        .collect(),
                )
                .await?;
            println!("subscribing {i} ({}) to topic..", router.endpoint().id());

            subscriptions.push(async move {
                if i < N_ALLOWED as usize {
                    let expected_neighbors = N_ALLOWED as usize - 1;
                    println!(
                        "waiting for {i} ({}) to connect to {expected_neighbors} neighbors..",
                        router.endpoint().id()
                    );
                    // Wait for all expected NeighborUp events before broadcasting.
                    let mut neighbor_count = 0;
                    tokio::time::timeout(Duration::from_secs(30), async {
                        while let Some(Ok(event)) = sub.next().await {
                            if matches!(event, Event::NeighborUp(_)) {
                                neighbor_count += 1;
                                if neighbor_count >= expected_neighbors {
                                    break;
                                }
                            }
                        }
                    })
                    .await
                    .expect("timed out waiting for all neighbors to connect");
                    println!("gossip connections {i} ready ({neighbor_count} neighbors)");
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
        let mut tasks = vec![];
        for (i, (_, mut gossip_rx)) in subscriptions.into_iter().enumerate() {
            tasks.push(tokio::spawn(async move {
                let expected_count = if i < N_ALLOWED as usize {
                    N_ALLOWED as usize - 1
                } else {
                    // Non-allowed clients shouldn't receive any messages
                    0
                };

                let mut received_messages = Vec::new();
                // For allowed clients, wait deterministically until all expected
                let timeout = if expected_count > 0 {
                    Duration::from_secs(30)
                } else {
                    Duration::from_secs(1)
                };
                tokio::time::timeout(timeout, async {
                    while let Some(Ok(msg)) = gossip_rx.next().await {
                        if let Event::Received(Message { content, .. }) = msg {
                            let message =
                                String::from_utf8(content.to_vec()).expect("non-utf8 message");
                            received_messages.push(message);
                            if received_messages.len() >= expected_count {
                                break;
                            }
                        } else if let Event::Lagged = msg {
                            panic!("lagged..");
                        }
                    }
                })
                .await
                .ok();

                // Verify that messages from non-allowed clients are not received
                for message in &received_messages {
                    let sender_id = message
                        .strip_prefix("Message from client ")
                        .and_then(|n| n.parse::<u8>().ok())
                        .expect("Invalid message format");

                    assert!(
                        sender_id < N_ALLOWED,
                        "Router {i} received message from non-allowed client {sender_id}"
                    );
                }

                // Verify that all messages from allowed clients are received
                if i < N_ALLOWED as usize {
                    assert_eq!(
                        received_messages.len(),
                        expected_count,
                        "Router {i} didn't receive all allowed messages. only saw {received_messages:?}"
                    );
                    println!("Router {i} received all messages");
                }
            }));
        }
        for task in tasks {
            task.await.expect("panicked");
        }

        for (_, router, _) in &routers {
            router.shutdown().await.expect("router shutdown failed");
        }

        Ok(())
    }
}
