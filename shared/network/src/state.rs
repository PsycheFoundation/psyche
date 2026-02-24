use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    time::{Duration, Instant},
};

use iroh::EndpointId;

use crate::{P2PEndpointInfo, connection_monitor::PeerBandwidth, download::DownloadUpdate};

#[derive(Debug)]
pub struct State {
    pub endpoint_id: Option<EndpointId>,
    pub connection_info: Vec<P2PEndpointInfo>,
    pub bandwidth_tracker: BandwidthTracker,
    pub bandwidth_history: VecDeque<f64>,
    pub download_progesses: HashMap<iroh_blobs::Hash, DownloadUpdate>,
}

impl State {
    pub fn new(bandwidth_average_period: u64) -> Self {
        Self {
            endpoint_id: Default::default(),
            connection_info: Default::default(),
            bandwidth_tracker: BandwidthTracker::new(bandwidth_average_period),
            bandwidth_history: Default::default(),
            download_progesses: Default::default(),
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
    events: HashMap<EndpointId, VecDeque<DownloadEvent>>,
}

impl BandwidthTracker {
    pub fn new(average_period_secs: u64) -> Self {
        BandwidthTracker {
            average_period_secs,
            events: HashMap::new(),
        }
    }

    pub fn add_event(&mut self, from: EndpointId, num_bytes: u64) {
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

    pub fn get_total_bandwidth(&self) -> f64 {
        self.events.values().map(endpoint_bandwidth).sum()
    }

    pub fn get_peer_bandwidth(&self, peer: &EndpointId) -> PeerBandwidth {
        match self.events.get(peer) {
            None => PeerBandwidth::NotMeasured,
            Some(events) => PeerBandwidth::Measured(endpoint_bandwidth(events)),
        }
    }
}

fn endpoint_bandwidth(val: &VecDeque<DownloadEvent>) -> f64 {
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
