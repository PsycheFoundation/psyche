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
        // Only track events with actual bytes transferred.
        // Zero-byte events (TryProvider, PartComplete, ProviderFailed) are noise.
        if num_bytes == 0 {
            return;
        }
        let now = Instant::now();
        let events = self.events.entry(from).or_default();
        events.push_back(DownloadEvent {
            timestamp: now,
            num_bytes,
        });
        Self::prune_stale(events, now, self.average_period_secs);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn get_total_bandwidth(&self) -> f64 {
        let max_age = Duration::from_secs(self.average_period_secs);
        let now = Instant::now();
        self.events
            .values()
            .map(|events| endpoint_bandwidth(events, now, max_age))
            .sum()
    }

    pub fn get_peer_bandwidth(&self, peer: &EndpointId) -> PeerBandwidth {
        let max_age = Duration::from_secs(self.average_period_secs);
        let now = Instant::now();
        match self.events.get(peer) {
            None => PeerBandwidth::NotMeasured,
            Some(events) if events.is_empty() => PeerBandwidth::NotMeasured,
            Some(events) => {
                // If the newest event is older than the window, all data is stale
                if now.duration_since(events.back().unwrap().timestamp) > max_age {
                    return PeerBandwidth::NotMeasured;
                }
                let bw = endpoint_bandwidth(events, now, max_age);
                if bw > 0.0 {
                    PeerBandwidth::Measured(bw)
                } else {
                    PeerBandwidth::NotMeasured
                }
            }
        }
    }

    fn prune_stale(events: &mut VecDeque<DownloadEvent>, now: Instant, max_age_secs: u64) {
        let max_age = Duration::from_secs(max_age_secs);
        while let Some(event) = events.front() {
            if now.duration_since(event.timestamp) > max_age {
                events.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Compute bandwidth in bytes/sec using the time span between the first and
/// last event, not `now`. This prevents idle time after a download from
/// diluting the measurement — the reading freezes at the last observed rate
/// until the events expire from the window.
fn endpoint_bandwidth(val: &VecDeque<DownloadEvent>, now: Instant, max_age: Duration) -> f64 {
    // Need at least 2 events to compute a rate between them
    if val.len() < 2 {
        return 0.0;
    }

    // Only consider events within the window
    let cutoff = now - max_age;
    let total_bytes: u64 = val
        .iter()
        .filter(|e| e.timestamp >= cutoff)
        .map(|e| e.num_bytes)
        .sum();
    let first_in_window = match val.iter().find(|e| e.timestamp >= cutoff) {
        Some(e) => e,
        None => return 0.0,
    };
    let last = val.back().unwrap();
    let seconds = last
        .timestamp
        .duration_since(first_in_window.timestamp)
        .as_secs_f64();

    if seconds > 0.0 {
        total_bytes as f64 / seconds
    } else {
        0.0
    }
}
