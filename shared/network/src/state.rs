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

/// Compute bandwidth in bytes/sec for events within the time window.
///
/// Uses bytes transferred *after* the first event divided by the elapsed time
/// from the first event to `now`. The first event's bytes are excluded from the
/// numerator because no time has elapsed when it arrives (fencepost correction).
///
/// Using `now` (instead of the last event's timestamp) as the time endpoint
/// ensures that congestion pauses between bursts are reflected in the
/// measurement. Without this, bursty relay traffic would report burst-rate
/// bandwidth (e.g. 8 MB/s) even when effective sustained throughput is far
/// lower (e.g. 500 KB/s).
fn endpoint_bandwidth(val: &VecDeque<DownloadEvent>, now: Instant, max_age: Duration) -> f64 {
    // Need at least 2 events to compute a rate between them
    if val.len() < 2 {
        return 0.0;
    }

    // Only consider events within the window
    let cutoff = now - max_age;
    let mut in_window = val.iter().filter(|e| e.timestamp >= cutoff).peekable();

    let first_in_window = match in_window.peek() {
        Some(e) => *e,
        None => return 0.0,
    };

    // Sum bytes from all events AFTER the first (exclude the first event's bytes)
    let first_timestamp = first_in_window.timestamp;
    let bytes_after_first: u64 = in_window.skip(1).map(|e| e.num_bytes).sum();

    if bytes_after_first == 0 {
        return 0.0;
    }

    // Use `now` as the end point so congestion pauses dilute the measurement
    let seconds = now.duration_since(first_timestamp).as_secs_f64();

    if seconds > 0.0 {
        bytes_after_first as f64 / seconds
    } else {
        0.0
    }
}
