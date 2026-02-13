use futures_util::StreamExt;
use futures_util::stream::{self};
use iroh::EndpointId;
use iroh_blobs::HashAndFormat;
use iroh_blobs::api::downloader::ContentDiscovery;
use n0_future::stream::Boxed;
use std::time::Duration;
use tracing::debug;

use crate::connection_monitor::ConnectionMonitor;

/// A ContentDiscovery implementation that orders providers by ascending connection latency.
#[derive(Debug, Clone)]
pub struct LatencySorted {
    nodes: Vec<EndpointId>,
    connection_monitor: ConnectionMonitor,
}

impl LatencySorted {
    pub fn new(nodes: Vec<EndpointId>, connection_monitor: ConnectionMonitor) -> Self {
        let mut seen = std::collections::HashSet::new();
        let unique_nodes: Vec<EndpointId> = nodes
            .into_iter()
            .filter(|node| seen.insert(*node))
            .collect();

        Self {
            nodes: unique_nodes,
            connection_monitor,
        }
    }
}

impl ContentDiscovery for LatencySorted {
    /// Finds providers for the given hash, sorted by ascending latency, without duplicates.
    fn find_providers(&self, _hash: HashAndFormat) -> Boxed<EndpointId> {
        // Collect latency information for each node (duplicates already removed in constructor)
        let mut nodes_with_latency: Vec<_> = self
            .nodes
            .iter()
            .map(|&node| {
                let latency = self
                    .connection_monitor
                    .get_connection(&node)
                    .and_then(|conn| conn.latency())
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
        let sorted_nodes: Vec<EndpointId> = nodes_with_latency
            .into_iter()
            .map(|(node, _)| node)
            .collect();

        debug!("[ContentDiscovery] Sorted nodes by latency: {sorted_nodes:?}");
        stream::iter(sorted_nodes).boxed()
    }
}
