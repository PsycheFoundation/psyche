use anyhow::Result;
use iroh::{NodeAddr, RelayMode};
use iroh_blobs::ticket::BlobTicket;
use psyche_metrics::ClientMetrics;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{join, select, time::timeout};
use tokio::{
    sync::{
        Mutex,
        mpsc::{self, UnboundedReceiver, UnboundedSender},
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    DiscoveryMode, DownloadType, NetworkConnection, NetworkEvent, PeerList, allowlist,
    psyche_relay_map,
};

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    Message { text: String },
    DistroResult { blob_ticket: BlobTicket, step: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
struct DistroResultBlob {
    step: u32,
    data: Vec<u8>,
}

type NC = NetworkConnection<Message, DistroResultBlob>;

#[derive(Debug)]
struct App {
    cancel: CancellationToken,
    current_step: u32,
    network: NC,
    our_id: NodeAddr,
    should_wait_before: bool,
    sender: bool,
    tx_waiting_for_download: Option<UnboundedSender<String>>,
    tx_retrying_download: Option<UnboundedSender<String>>,
}

impl App {
    async fn run(&mut self) {
        if self.sender {
            tokio::time::sleep(Duration::from_secs(10)).await;
            self.send().await;
        }
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    println!("Node Cancelled");
                    break;
                }
                event = self.network.poll_next() => {
                    match event {
                        Ok(event) => {
                            if let Some(event) = event {
                                self.on_network_event(event).await;
                            }
                        }
                        Err(err) => {
                            error!("Network error: {err}");
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn on_network_event(&mut self, event: NetworkEvent<Message, DistroResultBlob>) {
        match event {
            NetworkEvent::MessageReceived((from, Message::Message { text })) => {
                info!(name:"message_recv_text", from=from.fmt_short(), text=text)
            }
            NetworkEvent::MessageReceived((_, Message::DistroResult { step, blob_ticket })) => {
                let peers: Vec<NodeAddr> = self
                    .network
                    .get_all_peers()
                    .await
                    .iter()
                    .map(|(peer, _)| peer.clone())
                    .filter(|peer| peer.clone() != self.our_id)
                    .collect();

                if self.should_wait_before {
                    println!("Waiting to download");
                    if let Some(tx) = self.tx_waiting_for_download.take() {
                        let _ = tx.send("aborting".to_string());
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    println!("Downloading");
                }

                self.network
                    .start_download(vec![blob_ticket], step, DownloadType::DistroResult(peers));

                if !self.should_wait_before {
                    println!("Waiting to kill sender");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if let Some(tx) = self.tx_waiting_for_download.take() {
                        let _ = tx.send("aborting".to_string());
                    }
                }
            }
            NetworkEvent::DownloadComplete(_) => {
                println!("Download complete");
            }
            NetworkEvent::DownloadFailed(result) => {
                if let Some(tx) = self.tx_retrying_download.take() {
                    let _ = tx.send("download failed".to_string());
                }
                println!(
                    "Download failed: {}! Reason: {}",
                    result.blob_ticket.hash(),
                    result.error
                )
            }
            _ => {
                // Handle other events or explicitly ignore them
                println!("Unhandled network event");
            }
        }
    }

    async fn send(&mut self) {
        const DATA_SIZE_MB: usize = 1000;
        let data = vec![0u8; DATA_SIZE_MB * 1024 * 1024];
        let step = self.current_step;

        let blob_ticket = match self
            .network
            .add_downloadable(DistroResultBlob { step, data }, step)
            .await
        {
            Ok(bt) => {
                println!("Uploaded blob");
                bt
            }
            Err(e) => {
                println!("Couldn't add downloadable for step {step}. {e}");
                return;
            }
        };

        let message = Message::DistroResult {
            step,
            blob_ticket: blob_ticket.clone(),
        };

        if let Err(e) = self.network.broadcast(&message) {
            println!("Error sending message: {e}");
        } else {
            println!("broadcasted message for step {step}: {blob_ticket}");
        }
    }
}

async fn spawn_new_node(
    is_sender: bool,
    peer_list: Option<PeerList>,
    should_wait_to_download: bool,
    cancel_token: CancellationToken,
) -> Result<(
    Option<UnboundedReceiver<String>>,
    Option<UnboundedReceiver<String>>,
    PeerList,
    JoinHandle<()>,
)> {
    let (tx_waiting_for_download, rx_waiting_for_download) = if !is_sender {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let (tx_retrying_download, rx_retrying_download) = if !is_sender {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let PeerList(peers) = peer_list.unwrap_or_default();

    println!("joining gossip room");

    let network = NC::init(
        "test",
        None,
        None,
        RelayMode::Custom(psyche_relay_map()),
        DiscoveryMode::Local,
        peers,
        None,
        allowlist::AllowAll,
        20,
        Arc::new(ClientMetrics::new(None)),
    )
    .await?;

    let node_addr = network.router().endpoint().node_addr().await.unwrap();
    let join_id = PeerList(vec![node_addr]);

    let our_id = network
        .get_all_peers()
        .await
        .first()
        .map(|(addr, _)| addr.clone())
        .ok_or_else(|| anyhow::anyhow!("No peers found"))?;

    let mut app = App {
        cancel: cancel_token,
        current_step: 0,
        network,
        our_id,
        should_wait_before: should_wait_to_download,
        tx_waiting_for_download,
        sender: is_sender,
        tx_retrying_download,
    };

    let handle = tokio::spawn(async move {
        app.run().await;
        println!("Node finished running");
    });

    Ok((
        rx_waiting_for_download,
        rx_retrying_download,
        join_id,
        handle,
    ))
}

#[tokio::test]
async fn test_retry_connection() -> Result<()> {
    println!("Spawning first node (sender)");
    let sender_cancel = CancellationToken::new();
    let (_, _, join_id, handle_1) = spawn_new_node(
        true,  // is_sender
        None,  // peer_list
        false, // wait
        sender_cancel.clone(),
    )
    .await?;

    // Give the first node time to initialize
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("Spawning second node (receiver)");

    let receiver_cancel = CancellationToken::new();
    let (rx_waiting, rx_retrying, _, handle_2) = spawn_new_node(
        false, // is_sender
        Some(join_id),
        true, // wait
        receiver_cancel.clone(),
    )
    .await?;

    // Set up test completion logic
    let test_completed = Arc::new(Mutex::new(false));
    let test_completed_clone = test_completed.clone();

    // Handle waiting for download signal
    if let Some(mut rx) = rx_waiting {
        let sender_cancel_clone = sender_cancel.clone();
        tokio::spawn(async move {
            if rx.recv().await.is_some() {
                println!("ABORTING SENDER NODE");
                sender_cancel_clone.cancel();
            }
        });
    }

    // Handle retry signal (test completion)
    if let Some(mut rx) = rx_retrying {
        tokio::spawn(async move {
            if rx.recv().await.is_some() {
                println!("TEST PASSED - Retry detected");
                let mut completed = test_completed_clone.lock().await;
                *completed = true;
            }
        });
    }

    // Wait for test completion with timeout
    let test_duration = Duration::from_secs(40);
    let result = timeout(test_duration, async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let completed = test_completed.lock().await;
            if *completed {
                break;
            }
        }
    })
    .await;

    // Clean up
    sender_cancel.cancel();
    receiver_cancel.cancel();

    match result {
        Ok(_) => {
            println!("Test completed successfully");
            let _ = join!(handle_1, handle_2);
            Ok(())
        }
        Err(_) => {
            error!("Test timed out after {} seconds", test_duration.as_secs());
            let _ = join!(handle_1, handle_2);
            Err(anyhow::anyhow!("Test timed out"))
        }
    }
}

#[tokio::test]
async fn test_retry_connection_mid_download() -> Result<()> {
    println!("SPAWNING FIRST NODE (SENDER)");

    let sender_cancel = CancellationToken::new();
    let (_, _, join_id, handle_1) = spawn_new_node(
        true,  // is_sender
        None,  // peer_list
        false, // wait
        sender_cancel.clone(),
    )
    .await?;

    // Give the first node time to initialize
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("SPAWNING SECOND NODE (RECEIVER)");

    let receiver_cancel = CancellationToken::new();
    let (rx_waiting, rx_retrying, _, handle_2) = spawn_new_node(
        false, // is_sender
        Some(join_id),
        false, // wait
        receiver_cancel.clone(),
    )
    .await?;

    // Set up test completion logic
    let test_completed = Arc::new(Mutex::new(false));
    let test_completed_clone = test_completed.clone();

    // Handle waiting for download signal
    if let Some(mut rx) = rx_waiting {
        let sender_cancel_clone = sender_cancel.clone();
        tokio::spawn(async move {
            if rx.recv().await.is_some() {
                println!("ABORTING SENDER NODE");
                sender_cancel_clone.cancel();
            }
        });
    }

    // Handle retry signal (test completion)
    if let Some(mut rx) = rx_retrying {
        tokio::spawn(async move {
            if rx.recv().await.is_some() {
                println!("TEST PASSED - Retry detected");
                let mut completed = test_completed_clone.lock().await;
                *completed = true;
            }
        });
    }

    // Wait for test completion with timeout
    let test_duration = Duration::from_secs(50);
    let result = timeout(test_duration, async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let completed = test_completed.lock().await;
            if *completed {
                break;
            }
        }
    })
    .await;

    // Clean up
    sender_cancel.cancel();
    receiver_cancel.cancel();

    match result {
        Ok(_) => {
            println!("Test completed successfully");
            let _ = join!(handle_1, handle_2);
            Ok(())
        }
        Err(_) => {
            error!("Test timed out after {} seconds", test_duration.as_secs());
            let _ = join!(handle_1, handle_2);
            Err(anyhow::anyhow!("Test timed out"))
        }
    }
}
