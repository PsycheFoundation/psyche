use futures_util::{Stream, stream};
use iroh::EndpointId;
use iroh::discovery::{DiscoveryError, DiscoveryItem};
use iroh::endpoint_info::{EndpointData, EndpointInfo};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::error;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

#[derive(Debug)]
pub(crate) struct LocalTestDiscovery(EndpointId);

#[derive(Serialize, Deserialize)]
struct StoredNodeInfo {
    relay_url: Option<String>,
    direct_addresses: Vec<SocketAddr>,
}

impl LocalTestDiscovery {
    pub fn new(endpoint_id: EndpointId) -> Self {
        Self(endpoint_id)
    }
    fn get_discovery_dir() -> PathBuf {
        PathBuf::from("/tmp/iroh_local_discovery")
    }

    fn get_node_file_path(endpoint_id: &EndpointId) -> PathBuf {
        Self::get_discovery_dir().join(endpoint_id.to_string())
    }
}

impl iroh::discovery::Discovery for LocalTestDiscovery {
    fn publish(&self, data: &EndpointData) {
        // Create discovery directory if it doesn't exist
        let discovery_dir = Self::get_discovery_dir();
        fs::create_dir_all(&discovery_dir).expect("Failed to create discovery directory");

        // Prepare node info for storage
        let relay = data.relay_urls().next();
        let relay_str: Option<String>;
        match relay {
            Some(url) => relay_str = Some(url.to_string()),
            None => relay_str = None,
        }
        let node_info = StoredNodeInfo {
            relay_url: relay_str,
            direct_addresses: data.ip_addrs().cloned().collect(),
        };

        // Serialize and write to file
        let file_path = Self::get_node_file_path(&self.0);
        let content = serde_json::to_string(&node_info).expect("Failed to serialize node info");
        fs::write(file_path, content).expect("Failed to write node info to file");
    }

    fn resolve(
        &self,
        endpoint_id: iroh::EndpointId,
    ) -> Option<BoxStream<anyhow::Result<DiscoveryItem, DiscoveryError>>> {
        let file_path = Self::get_node_file_path(&endpoint_id);

        if !file_path.exists() {
            error!("no local node filepath found for node id {endpoint_id} at {file_path:?}");
            return None;
        }

        // Read and parse the stored node info
        let content = match fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(_) => return None,
        };

        let node_info: StoredNodeInfo = match serde_json::from_str(&content) {
            Ok(info) => info,
            Err(_) => return None,
        };

        // Convert the stored info into a DiscoveryItem
        let relay_url = node_info
            .relay_url
            .and_then(|url| url.parse::<iroh::RelayUrl>().ok());

        let direct_addresses: BTreeSet<_> = node_info.direct_addresses.into_iter().collect();

        let discovery_item = iroh::discovery::DiscoveryItem::new(
            EndpointInfo {
                endpoint_id,
                data: EndpointData::new(None)
                    .with_relay_url(relay_url)
                    .with_ip_addrs(direct_addresses),
            },
            "local_test_discovery",
            Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
            ),
        );

        // Return a single-item stream
        Some(Box::pin(stream::once(async move { Ok(discovery_item) })))
    }
}
