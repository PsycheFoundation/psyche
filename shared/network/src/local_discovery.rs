use futures_util::{stream, Stream};
use iroh::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

#[derive(Debug)]
pub(crate) struct LocalTestDiscovery(NodeId);

#[derive(Serialize, Deserialize)]
struct StoredNodeInfo {
    relay_url: Option<String>,
    direct_addresses: Vec<SocketAddr>,
}

impl LocalTestDiscovery {
    pub fn new(node_id: NodeId) -> Self {
        Self(node_id)
    }
    fn get_discovery_dir() -> PathBuf {
        PathBuf::from("/tmp/iroh_local_discovery")
    }

    fn get_node_file_path(node_id: &NodeId) -> PathBuf {
        Self::get_discovery_dir().join(node_id.to_string())
    }
}

impl iroh::discovery::Discovery for LocalTestDiscovery {
    fn publish(&self, url: Option<&iroh::RelayUrl>, addrs: &BTreeSet<SocketAddr>) {
        // Create discovery directory if it doesn't exist
        let discovery_dir = Self::get_discovery_dir();
        fs::create_dir_all(&discovery_dir).expect("Failed to create discovery directory");

        // Prepare node info for storage
        let node_info = StoredNodeInfo {
            relay_url: url.map(|u| u.to_string()),
            direct_addresses: addrs.iter().cloned().collect(),
        };

        // Serialize and write to file
        let file_path = Self::get_node_file_path(&self.0);
        let content = serde_json::to_string(&node_info).expect("Failed to serialize node info");
        fs::write(file_path, content).expect("Failed to write node info to file");
    }

    fn resolve(
        &self,
        _endpoint: iroh::Endpoint,
        node_id: iroh::NodeId,
    ) -> Option<BoxStream<anyhow::Result<iroh::discovery::DiscoveryItem>>> {
        let file_path = Self::get_node_file_path(&node_id);

        if !file_path.exists() {
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

        let node_addr = NodeAddr {
            node_id,
            relay_url,
            direct_addresses,
        };

        let discovery_item = iroh::discovery::DiscoveryItem {
            node_addr,
            provenance: "local_test_discovery",
            last_updated: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
            ),
        };

        // Return a single-item stream
        Some(Box::pin(stream::once(async move { Ok(discovery_item) })))
    }

    fn subscribe(&self) -> Option<BoxStream<iroh::discovery::DiscoveryItem>> {
        None
    }
}
