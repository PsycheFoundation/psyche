use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    time::{Duration, Instant},
};

use iroh::NodeId;

use crate::{P2PNodeInfo, download_manager::DownloadUpdate};

#[derive(Debug)]
pub struct State {
    pub node_id: Option<NodeId>,
    pub node_connections: Vec<P2PNodeInfo>,
    pub bandwidth_tracker: BandwidthTracker,
    pub bandwidth_history: VecDeque<f64>,
    pub download_progresses: HashMap<iroh_blobs::Hash, DownloadUpdate>,
}

impl State {
    pub fn new(bandwidth_average_period: u64) -> Self {
        Self {
            node_id: Default::default(),
            node_connections: Default::default(),
            bandwidth_tracker: BandwidthTracker::new(bandwidth_average_period),
            bandwidth_history: Default::default(),
            download_progresses: Default::default(),
        }
    }
}

#[derive(Debug)]
struct DownloadEvent {
    timestamp: Instant,
    num_bytes: u64,
}

#[derive(Debug)]
pub struct BandwidthTracker {
    average_period_secs: u64,
    events: HashMap<NodeId, VecDeque<DownloadEvent>>,
}

impl BandwidthTracker {
    pub fn new(average_period_secs: u64) -> Self {
        BandwidthTracker {
            average_period_secs,
            events: HashMap::new(),
        }
    }

    pub fn add_event(&mut self, from: NodeId, num_bytes: u64) {
        let now = Instant::now();
        let events = self.events.entry(from).or_default();
        events.push_back(DownloadEvent {
            timestamp: now,
            num_bytes,
        });

        while let Some(event) = events.front() {
            if now.duration_since(event.timestamp) > Duration::from_secs(self.average_period_secs) {
                events.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn get_bandwidth_by_node(&self, id: &NodeId) -> Option<f64> {
        self.events.get(id).map(node_bandwidth)
    }

    pub fn get_total_bandwidth(&self) -> f64 {
        self.events.values().map(node_bandwidth).sum()
    }
}

fn node_bandwidth(val: &VecDeque<DownloadEvent>) -> f64 {
    if val.is_empty() {
        return 0.0;
    }
    let duration = Instant::now().duration_since(val.front().unwrap().timestamp);
    let total_bytes: u64 = val.iter().map(|v| v.num_bytes).sum();
    let seconds = duration.as_secs_f64();

    if seconds > 0.0 {
        total_bytes as f64 / seconds
    } else {
        0.0
    }
}
