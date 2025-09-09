use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Debug,
    time::{Duration, Instant},
};

use iroh::{NodeId, PublicKey, endpoint::ConnectionType};

use crate::{download_manager::DownloadUpdate, peer_list::PeerList};

#[derive(Debug)]
pub struct State {
    pub join_ticket: PeerList,
    pub last_seen: HashMap<PublicKey, (ConnectionType, Instant)>,
    pub bandwidth_tracker: BandwidthTracker,
    pub bandwidth_history: VecDeque<f64>,
    pub download_progesses: HashMap<iroh_blobs::Hash, DownloadUpdate>,

    pub currently_sharing_blobs: HashSet<iroh_blobs::Hash>,
    pub blob_tags: HashSet<(u32, iroh_blobs::Hash)>,
}

impl State {
    pub fn new(bandwidth_average_period: u64) -> Self {
        Self {
            join_ticket: Default::default(),
            last_seen: Default::default(),
            bandwidth_tracker: BandwidthTracker::new(bandwidth_average_period),
            bandwidth_history: Default::default(),
            download_progesses: Default::default(),
            currently_sharing_blobs: Default::default(),
            blob_tags: Default::default(),
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
