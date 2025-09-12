use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use iroh::{Endpoint, NodeId};
use iroh_blobs::HashAndFormat;
use iroh_blobs::api::downloader::ContentDiscovery;
use std::time::Duration;
use tracing::{debug, trace};

/// A ContentDiscovery implementation that orders providers by ascending connection latency.
#[derive(Debug)]
pub struct LatencySorted {
    nodes: Vec<NodeId>,
    endpoint: Endpoint,
}

impl LatencySorted {
    pub fn new(nodes: Vec<NodeId>, endpoint: Endpoint) -> Self {
        Self { nodes, endpoint }
    }
}

impl ContentDiscovery for LatencySorted {
    fn find_providers(&self, _hash: HashAndFormat) -> BoxStream<'static, NodeId> {
        // Collect latency information for each node
        let mut nodes_with_latency: Vec<_> = self
            .nodes
            .iter()
            .map(|&node| {
                let latency = self
                    .endpoint
                    .remote_info(node)
                    .and_then(|info| info.latency)
                    .unwrap_or(Duration::MAX); // Unknown nodes get max latency

                debug!(
                    "[ContentDiscovery] Node {} latency: {}ms",
                    node,
                    if latency == Duration::MAX {
                        "unknown".to_string()
                    } else {
                        format!("{}", latency.as_millis())
                    }
                );

                (node, latency)
            })
            .collect();

        // Sort by latency, lowest first.
        nodes_with_latency.sort_by_key(|(_, latency)| *latency);
        let sorted_nodes: Vec<NodeId> = nodes_with_latency
            .into_iter()
            .map(|(node, _)| node)
            .collect();

        debug!("[ContentDiscovery] Sorted nodes by latency: {sorted_nodes:?}");
        stream::iter(sorted_nodes).boxed()
    }
}
