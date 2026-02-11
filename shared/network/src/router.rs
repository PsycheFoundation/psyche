use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::BlobsProtocol;
use iroh_gossip::net::Gossip;

use iroh::{
    Endpoint,
    protocol::{AccessLimit, ProtocolHandler, Router},
};

use crate::{Allowlist, ModelSharing, p2p_model_sharing};

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

pub(crate) fn spawn_router_with_allowlist<
    A: Allowlist + 'static + Send + std::marker::Sync,
    P: ProtocolHandler + Clone,
>(
    allowlist: A,
    endpoint: Endpoint,
    protocols: SupportedProtocols,
    additional_protocol: Option<(&'static [u8], P)>,
) -> Result<Arc<Router>> {
    let allowlist_clone = allowlist.clone();
    let allowlisted_blobs = AccessLimit::new(protocols.1, move |endpoint_id| {
        allowlist_clone.allowed(endpoint_id)
    });
    let allowlist_clone_2 = allowlist.clone();
    let allowlisted_gossip = AccessLimit::new(protocols.0.clone(), move |endpoint_id| {
        allowlist_clone_2.allowed(endpoint_id)
    });
    let allowlist_clone_3 = allowlist.clone();
    let allowlisted_model_sharing = AccessLimit::new(protocols.2.clone(), move |endpoint_id| {
        allowlist_clone_3.allowed(endpoint_id)
    });

    let mut builder = Router::builder(endpoint.clone())
        .accept(iroh_blobs::ALPN, allowlisted_blobs)
        .accept(iroh_gossip::ALPN, allowlisted_gossip)
        .accept(p2p_model_sharing::ALPN, allowlisted_model_sharing);

    // add optional custom protocol if provided
    if let Some((alpn, handler)) = additional_protocol {
        let allowlist_clone = allowlist.clone();
        let allowlisted_handler = AccessLimit::new(handler, move |endpoint_id| {
            allowlist_clone.allowed(endpoint_id)
        });
        builder = builder.accept(alpn, allowlisted_handler);
    }

    let router = Arc::new(builder.spawn());

    Ok(router)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::future::join_all;
    use iroh::{Endpoint, SecretKey, address_lookup::memory::MemoryLookup};
    use iroh_blobs::store::mem::MemStore;
    use iroh_gossip::{
        api::{Event, Message},
        net::Gossip,
        proto::TopicId,
    };
    use rand::Fill;
    use tokio_stream::StreamExt;

    use crate::allowlist::{self, AllowDynamic};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_shutdown() -> Result<()> {
        let endpoint = Endpoint::builder().bind().await?;
        let blobs = MemStore::new();
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let (tx_model_parameter_req, _rx_model_parameter_req) =
            tokio::sync::mpsc::unbounded_channel();
        let (tx_model_config_req, _rx_model_config_req) = tokio::sync::mpsc::unbounded_channel();
        let p2p_model_sharing = ModelSharing::new(tx_model_parameter_req, tx_model_config_req);
        let allowlist = allowlist::AllowAll;
        let blobs_protocol = BlobsProtocol::new(&blobs, None);
        let router = spawn_router_with_allowlist::<_, iroh_gossip::net::Gossip>(
            allowlist.clone(),
            endpoint.clone(),
            SupportedProtocols::new(gossip.clone(), blobs_protocol, p2p_model_sharing),
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
    #[test_log::test(tokio::test)]
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
                        .clear_address_lookup()
                        .address_lookup(static_discovery.clone())
                        .bind()
                        .await?;
                    let gossip = Gossip::builder().spawn(endpoint.clone());

                    let allowlisted_gossip = AccessLimit::new(gossip.clone(), move |endpoint_id| {
                        allowlist.allowed(endpoint_id)
                    });
                    let router = Arc::new(
                        Router::builder(endpoint.clone())
                            .accept(iroh_gossip::ALPN, allowlisted_gossip)
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
                    println!(
                        "waiting for {i} ({}) to get at least 1 peer..",
                        router.endpoint().id()
                    );
                    sub.joined().await.unwrap();
                    // long delay to ensure gossip is fully connected.
                    // if we don't wait until we have a fully connected gossip, then we could broadcast while we only had unidirectional neighbor connections:
                    // confirming a join takes one trip, so we can end in this situation when broadcasting starts:
                    // a: neighbors = [b]
                    // b: neighbors = [c]
                    // c: neighbors = [b]
                    // now c broadcasts, sends to b, and b drops the message because it does not have further neighbors
                    // in a very short time after, it will receive the neighbor message from a, but too late, because iroh-gossip does not forward messages received before a join

                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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
                if let Event::Received(Message { content, .. }) = msg {
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
