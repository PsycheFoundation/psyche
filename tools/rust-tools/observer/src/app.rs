use std::collections::{HashSet, VecDeque};

use indexmap::IndexMap;
use psyche_event_sourcing::projection::{ClusterProjection, ClusterSnapshot};
use psyche_event_sourcing::timeline::{ClusterTimeline, TimelineEntry};
use strum::{Display, EnumIter, IntoEnumIterator};

pub use crate::widgets::waterfall::{EventCategory, entry_matches_filter};

#[derive(Debug, Clone, Default)]
pub struct NodeFileStats {
    /// Total bytes of all .postcard files on disk for this node.
    pub total_bytes: u64,
    /// Average write rate: total_bytes / (last_event_time - first_event_time).
    pub bytes_per_sec: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, Display)]
pub enum DetailPanel {
    Event,
    Batches,
    Node,
    Loss,
}

pub struct App {
    pub timeline: ClusterTimeline,
    pub cursor: usize,
    /// None = "all nodes" view; Some(i) = node at index i in snapshot.nodes.
    pub selected_node_idx: Option<usize>,
    /// Vertical scroll offset for the node/waterfall rows.
    pub node_scroll: usize,
    pub playing: bool,
    pub speed: u64,

    /// Live file-size stats per node_id, refreshed every tick.
    pub node_file_stats: IndexMap<String, NodeFileStats>,

    /// Which sub-panel is open
    pub detail_panel: DetailPanel,

    /// Number of timeline entries visible at once in the waterfall x-axis.
    pub waterfall_zoom: usize,
    /// Index of the first timeline entry visible in the waterfall window.
    pub waterfall_x_scroll: usize,
    /// Set of active category filters. Events match if their category is in this set.
    /// Empty set means show all events.
    pub waterfall_filter: HashSet<EventCategory>,

    /// Live projection kept at the current cursor position. Accessing the
    /// current snapshot is a free reference into this projection — no cloning.
    /// Lazily initialized / re-synced by `sync_projection()`.
    projection: Option<(usize, ClusterProjection)>,
    /// Recent snapshots for O(1) backward stepping.
    /// Back = snapshot at (projection_pos − 1), front = oldest.
    backward_cache: VecDeque<ClusterSnapshot>,

    /// Tick counter — used to throttle expensive I/O operations.
    tick_count: u64,
}

impl App {
    pub fn new(timeline: ClusterTimeline) -> Self {
        let len = timeline.len();
        Self {
            timeline,
            cursor: len.saturating_sub(1),
            selected_node_idx: None,
            node_scroll: 0,
            playing: false,
            speed: 1,
            node_file_stats: IndexMap::new(),
            detail_panel: DetailPanel::Event,
            waterfall_zoom: 20,
            waterfall_x_scroll: 0,
            waterfall_filter: HashSet::new(),
            projection: None,
            backward_cache: VecDeque::new(),
            tick_count: 0,
        }
    }

    /// Max snapshots kept in the backward cache for O(1) backward stepping.
    const BACKWARD_CACHE_SIZE: usize = 50;

    /// Ensure the projection is synced, then return an immutable reference.
    /// Prefer calling `sync_projection()` once, then `snapshot()` to avoid
    /// the `&mut self` borrow persisting through the reference lifetime.
    #[doc(hidden)]
    #[allow(unused)]
    pub fn current_snapshot(&mut self) -> &ClusterSnapshot {
        self.sync_projection();
        self.projection.as_ref().unwrap().1.snapshot()
    }

    /// Return the current snapshot without syncing. Panics if `sync_projection`
    /// hasn't been called yet (projection is None).
    pub fn snapshot(&self) -> &ClusterSnapshot {
        self.projection.as_ref().unwrap().1.snapshot()
    }

    /// Ensure the projection is at the current cursor position.
    ///
    /// Three paths, cheapest first:
    /// 1. Already synced → free.
    /// 2. Small forward delta (≤ BACKWARD_CACHE_SIZE) → apply entries, O(1) each.
    /// 3. Backward within cache → pop from backward_cache, O(1).
    /// 4. Large jump / cold start → replay from nearest checkpoint.
    pub fn sync_projection(&mut self) {
        let cursor = self.cursor;
        let total = self.timeline.len();

        if total == 0 {
            self.projection = Some((0, ClusterProjection::new()));
            self.backward_cache.clear();
            return;
        }
        let cursor = cursor.min(total - 1);

        // Take the projection out so we can freely access other self fields.
        let Some((pp, mut proj)) = self.projection.take() else {
            // Cold start — replay from checkpoint.
            let snap = self.timeline.snapshot_at(cursor);
            self.projection = Some((cursor, ClusterProjection::from_snapshot(snap)));
            self.backward_cache.clear();
            return;
        };

        if pp == cursor {
            self.projection = Some((pp, proj));
            return;
        }

        // Small forward step — walk forward, pushing snapshots to backward_cache.
        if cursor > pp && cursor - pp <= Self::BACKWARD_CACHE_SIZE {
            let entries = self.timeline.entries();
            for entry in entries.iter().take(cursor + 1).skip(pp + 1) {
                self.backward_cache.push_back(proj.snapshot().clone());
                if self.backward_cache.len() > Self::BACKWARD_CACHE_SIZE {
                    self.backward_cache.pop_front();
                }
                Self::apply_entry(&mut proj, entry);
            }
            self.projection = Some((cursor, proj));
            return;
        }

        // Backward within cache.
        if cursor < pp {
            let steps_back = pp - cursor;
            if steps_back <= self.backward_cache.len() {
                let keep = self.backward_cache.len() - steps_back;
                let snap = self.backward_cache[keep].clone();
                self.backward_cache.truncate(keep);
                self.projection = Some((cursor, ClusterProjection::from_snapshot(snap)));
                return;
            }
        }

        // Large jump — replay from checkpoint.
        let snap = self.timeline.snapshot_at(cursor);
        self.projection = Some((cursor, ClusterProjection::from_snapshot(snap)));
        self.backward_cache.clear();
    }

    /// Apply a single timeline entry to a projection (mirrors ClusterTimeline::apply_entry).
    fn apply_entry(proj: &mut ClusterProjection, entry: &TimelineEntry) {
        match entry {
            TimelineEntry::Node { node_id, event, .. } => {
                proj.apply_node_event(node_id, event);
            }
            TimelineEntry::Coordinator { state, .. } => {
                proj.apply_coordinator(state.clone());
            }
        }
    }

    pub fn step_forward(&mut self, n: usize) {
        let max = self.timeline.len().saturating_sub(1);
        if self.waterfall_filter.is_empty() {
            self.cursor = (self.cursor + n).min(max);
        } else {
            let entries = self.timeline.entries();
            let mut remaining = n;
            let mut pos = self.cursor;
            while remaining > 0 && pos < max {
                pos += 1;
                if entry_matches_filter(&entries[pos], &self.waterfall_filter) {
                    remaining -= 1;
                }
            }
            self.cursor = pos;
        }
    }

    pub fn step_backward(&mut self, n: usize) {
        if self.waterfall_filter.is_empty() {
            self.cursor = self.cursor.saturating_sub(n);
        } else {
            let entries = self.timeline.entries();
            let mut remaining = n;
            let mut pos = self.cursor;
            while remaining > 0 && pos > 0 {
                pos -= 1;
                if entry_matches_filter(&entries[pos], &self.waterfall_filter) {
                    remaining -= 1;
                }
            }
            self.cursor = pos;
        }
    }

    pub fn go_first(&mut self) {
        self.cursor = 0;
    }

    pub fn go_last(&mut self) {
        self.cursor = self.timeline.len().saturating_sub(1);
    }

    /// Select next node (↓). Cycles: None → 0 → 1 → … → last → None.
    pub fn next_node(&mut self) {
        let count = self.timeline.all_entity_ids().len();
        if count == 0 {
            self.selected_node_idx = None;
            return;
        }
        self.selected_node_idx = match self.selected_node_idx {
            None => Some(0),
            Some(i) if i + 1 < count => Some(i + 1),
            Some(_) => None,
        };
    }

    /// Select previous node (↑). Cycles: None → last → … → 0 → None.
    pub fn prev_node(&mut self) {
        let count = self.timeline.all_entity_ids().len();
        if count == 0 {
            self.selected_node_idx = None;
            return;
        }
        self.selected_node_idx = match self.selected_node_idx {
            None => Some(count - 1),
            Some(0) => None,
            Some(i) => Some(i - 1),
        };
    }

    /// Adjust `node_scroll` so the selected node is within the visible viewport.
    /// The virtual row list has "all info" at index 0 and nodes at 1..=N.
    pub fn ensure_node_visible(&mut self, viewport_h: usize) {
        if viewport_h == 0 {
            return;
        }
        // Map selection to virtual row index: None → 0, Some(i) → i+1.
        let virt_idx = match self.selected_node_idx {
            None => 0,
            Some(i) => i + 1,
        };
        if virt_idx < self.node_scroll {
            self.node_scroll = virt_idx;
        } else if virt_idx >= self.node_scroll + viewport_h {
            self.node_scroll = virt_idx + 1 - viewport_h;
        }
    }

    pub fn toggle_play(&mut self) {
        self.playing = !self.playing;
    }

    pub fn set_speed(&mut self, speed: u64) {
        self.speed = speed;
    }

    /// Zoom in: halve the number of visible events (min 5).
    pub fn zoom_in(&mut self) {
        self.waterfall_zoom = (self.waterfall_zoom / 2).max(5);
        self.ensure_cursor_visible();
    }

    /// Zoom out: double the number of visible events.
    pub fn zoom_out(&mut self) {
        self.waterfall_zoom = (self.waterfall_zoom * 2).min(self.timeline.len().max(20));
        self.ensure_cursor_visible();
    }

    /// Adjust `waterfall_x_scroll` so the cursor stays within the visible window,
    /// and the window is always as full as possible (no empty space at the right).
    pub fn ensure_cursor_visible(&mut self) {
        let total = self.timeline.len();

        // Pull x_scroll left if there is room to the left that isn't being shown.
        // This makes zoom work correctly near the right end of the timeline.
        let max_scroll = total.saturating_sub(self.waterfall_zoom);
        self.waterfall_x_scroll = self.waterfall_x_scroll.min(max_scroll);

        // Then make sure the cursor itself is within the window.
        if self.cursor < self.waterfall_x_scroll {
            self.waterfall_x_scroll = self.cursor;
        } else if self.waterfall_zoom > 0
            && self.cursor >= self.waterfall_x_scroll + self.waterfall_zoom
        {
            self.waterfall_x_scroll = self.cursor + 1 - self.waterfall_zoom;
        }
    }

    /// Switch a detail panel.
    pub fn switch_panel(&mut self, panel: DetailPanel) {
        self.detail_panel = panel;
    }

    /// Toggle a category filter on the waterfall (press same key again to remove).
    pub fn toggle_category_filter(&mut self, cat: EventCategory) {
        if self.waterfall_filter.contains(&cat) {
            self.waterfall_filter.remove(&cat);
        } else {
            self.waterfall_filter.insert(cat);
        }
    }

    pub fn cycle_detail_panel(&mut self) {
        let mut iter = DetailPanel::iter().cycle();
        iter.find(|p| *p == self.detail_panel);
        self.detail_panel = iter.next().unwrap();
    }

    pub fn tick(&mut self) -> bool {
        self.tick_count += 1;

        // File stats are expensive (directory traversal + stat calls).
        // Only refresh every ~2s (10 ticks × 200ms).
        if self.tick_count.is_multiple_of(10) {
            self.refresh_file_stats();
        }

        // Pull in any new events written to disk since last tick.
        let was_at_tail =
            self.timeline.is_empty() || self.cursor >= self.timeline.len().saturating_sub(1);

        if self.timeline.refresh().unwrap_or(false) {
            // Invalidate projection + backward cache. The cost is a single
            // snapshot_at() replay on the next render — much cheaper than the
            // old 101-snapshot cache refill.
            self.projection = None;
            self.backward_cache.clear();

            if was_at_tail {
                // Auto-follow: stay pinned to the latest event.
                self.cursor = self.timeline.len().saturating_sub(1);
            }
        }

        if self.playing {
            let max = self.timeline.len().saturating_sub(1);
            if self.cursor < max {
                self.cursor += self.speed as usize;
                if self.cursor > max {
                    self.cursor = max;
                }
                return true;
            } else {
                self.playing = false;
            }
        }
        false
    }

    fn refresh_file_stats(&mut self) {
        let sizes = self.timeline.node_file_sizes();
        let ranges = self.timeline.node_timestamp_ranges();

        self.node_file_stats.clear();
        for (node_id, &total_bytes) in sizes {
            let bytes_per_sec = ranges
                .get(node_id)
                .map(|r| {
                    let duration_secs = (r.last - r.first).num_seconds();
                    if duration_secs > 0 {
                        total_bytes / duration_secs as u64
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            self.node_file_stats.insert(
                node_id.clone(),
                NodeFileStats {
                    total_bytes,
                    bytes_per_sec,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use psyche_core::{BatchId, ClosedInterval};
    use psyche_event_sourcing::events::{EpochStarted, Event, EventData, Train, train};
    use std::time::Instant;

    /// Build a timeline with `n` fake node events across `num_nodes` nodes.
    /// Mixes training events (which grow snapshot.nodes/losses) to make
    /// snapshots realistically heavy.
    fn make_timeline(n: usize, num_nodes: usize) -> ClusterTimeline {
        let mut timeline = ClusterTimeline::new();
        let base_ts = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();

        for i in 0..n {
            let node_id = format!("node-{}", i % num_nodes);
            let ts = base_ts + chrono::Duration::milliseconds(i as i64);
            let data = if i % 3 == 0 {
                // Training events grow the losses vec inside the snapshot.
                EventData::Train(Train::TrainingFinished(train::TrainingFinished {
                    batch_id: BatchId(ClosedInterval {
                        start: i as u64,
                        end: i as u64,
                    }),
                    step: (i / num_nodes) as u64,
                    loss: Some(1.0 / (1.0 + i as f64)),
                }))
            } else {
                EventData::EpochStarted(EpochStarted {
                    epoch_number: (i / 100) as u64,
                })
            };
            let event = Event {
                timestamp: ts,
                data,
            };
            timeline.push_node_event(node_id, event);
        }
        timeline
    }

    fn make_app(n: usize, num_nodes: usize) -> App {
        App::new(make_timeline(n, num_nodes))
    }

    fn time_op(label: &str, iterations: usize, mut f: impl FnMut()) {
        // Warmup
        f();
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        let elapsed = start.elapsed();
        let per_iter = elapsed / iterations as u32;
        eprintln!(
            "  {:<45} {:>8.2?} / iter  ({} iters, {:>8.2?} total)",
            label, per_iter, iterations, elapsed
        );
    }

    #[test]
    fn bench_snapshot_operations() {
        let events = 20_000;
        let nodes = 5;
        eprintln!("\n=== Snapshot Cache Benchmark ({events} events, {nodes} nodes) ===\n");

        // --- Cold start: first current_snapshot() call ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = events / 2;
            let start = Instant::now();
            let _ = app.current_snapshot();
            let cold = start.elapsed();
            eprintln!(
                "  {:<45} {:>8.2?}",
                "cold start (first snapshot at midpoint)", cold
            );
        }

        // --- Warm: repeated current_snapshot() at same position ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = events / 2;
            let _ = app.current_snapshot();
            time_op("current_snapshot (same pos, no-op)", 10_000, || {
                let _ = app.current_snapshot();
            });
        }

        // --- Step forward 1 ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = 0;
            let _ = app.current_snapshot();
            time_op("step forward 1 + snapshot", 500, || {
                app.step_forward(1);
                let _ = app.current_snapshot();
            });
        }

        // --- Step backward 1 (within cache) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = 250;
            let _ = app.current_snapshot();
            // Fill backward cache by stepping forward
            for _ in 0..50 {
                app.step_forward(1);
                let _ = app.current_snapshot();
            }
            // Now step backward — should hit cache
            time_op("step backward 1 (cached) + snapshot", 50, || {
                app.step_backward(1);
                let _ = app.current_snapshot();
            });
        }

        // --- Step backward 1 (cache miss — replay from checkpoint) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = events / 2;
            let _ = app.current_snapshot();
            time_op("step backward 1 (cold replay) + snapshot", 20, || {
                app.step_backward(1);
                app.backward_cache.clear(); // force cache miss each time
                let _ = app.current_snapshot();
            });
        }

        // --- Forward 50 (Shift+Arrow) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = 0;
            let _ = app.current_snapshot();
            time_op("step forward 50 (shift+arrow) + snapshot", 100, || {
                app.step_forward(50);
                let _ = app.current_snapshot();
            });
        }

        // --- Large jump (go_first / go_last) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = 0;
            let _ = app.current_snapshot();
            time_op("go_last + snapshot (large jump)", 20, || {
                app.go_last();
                let _ = app.current_snapshot();
            });
        }

        // --- Scrub back and forth (the main use case) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = events / 2;
            let _ = app.current_snapshot();
            // Fill backward cache
            for _ in 0..30 {
                app.step_forward(1);
                let _ = app.current_snapshot();
            }
            time_op("scrub: fwd 1 + snap, back 1 + snap", 500, || {
                app.step_forward(1);
                let _ = app.current_snapshot();
                app.step_backward(1);
                let _ = app.current_snapshot();
            });
        }

        // --- Playback at speed 20 ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = 0;
            let _ = app.current_snapshot();
            time_op("playback speed=20 + snapshot (per tick)", 100, || {
                app.step_forward(20);
                let _ = app.current_snapshot();
            });
        }

        // --- Clone cost: snapshot.clone() (what tui.rs does each frame) ---
        {
            let mut app = make_app(events, nodes);
            app.cursor = events / 2;
            let _ = app.current_snapshot();
            time_op("snapshot.clone() (tui render cost)", 1000, || {
                let _ = app.current_snapshot().clone();
            });
        }

        eprintln!();
    }

    #[test]
    fn bench_scaling() {
        eprintln!("\n=== Scaling Benchmark ===\n");

        for &(events, nodes) in &[(5_000, 5), (20_000, 5), (100_000, 5), (100_000, 20)] {
            eprintln!("  --- {events} events, {nodes} nodes ---");

            // Cold start at midpoint
            {
                let mut app = make_app(events, nodes);
                app.cursor = events / 2;
                let start = Instant::now();
                let _ = app.current_snapshot();
                let cold = start.elapsed();
                eprintln!("    {:<40} {:>10.2?}", "cold start (midpoint)", cold);
            }

            // Step forward 1
            {
                let mut app = make_app(events, nodes);
                app.cursor = events / 2;
                let _ = app.current_snapshot();
                time_op("  step forward 1 + snapshot", 200, || {
                    app.step_forward(1);
                    let _ = app.current_snapshot();
                });
            }

            // snapshot.clone()
            {
                let mut app = make_app(events, nodes);
                app.cursor = events / 2;
                let _ = app.current_snapshot();
                time_op("  snapshot.clone()", 200, || {
                    let _ = app.current_snapshot().clone();
                });
            }

            // Cold backward (cache miss)
            {
                let mut app = make_app(events, nodes);
                app.cursor = events / 2;
                let _ = app.current_snapshot();
                time_op("  backward 1 (cold replay)", 10, || {
                    app.step_backward(1);
                    app.backward_cache.clear();
                    let _ = app.current_snapshot();
                });
            }

            eprintln!();
        }
    }
}
