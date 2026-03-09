use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use psyche_coordinator::{CommitteeSelection, Coordinator, assign_data_for_state};

use crate::events::Event;
use crate::projection::{ClusterProjection, ClusterSnapshot, CoordinatorStateSnapshot};
use crate::store::try_decode_cobs_frame;

const CHECKPOINT_INTERVAL: usize = 5000;
/// Subdirectory name under the events dir that holds coordinator records.
const COORDINATOR_SUBDIR: &str = "coordinator";

/// Progress report emitted during [`ClusterTimeline::from_events_dir_with_progress`].
#[derive(Debug, Clone)]
pub struct LoadProgress {
    /// Current phase label, e.g. "scanning", "reading", "sorting", "indexing".
    pub phase: &'static str,
    /// Fraction complete within this phase, in `0.0..=1.0`.
    pub fraction: f32,
    /// Total bytes discovered on disk (available from "reading" phase onward).
    pub total_bytes: u64,
    /// Bytes read so far (meaningful during "reading" phase).
    pub bytes_read: u64,
    /// Total event entries decoded so far.
    pub entries: usize,
    /// Number of .postcard files found.
    pub files: usize,
}

/// Size of a single coordinator disk record: i64 timestamp + Coordinator bytes.
const COORD_RECORD_SIZE: usize = std::mem::size_of::<i64>() + std::mem::size_of::<Coordinator>();

fn decode_coordinator_records(data: &[u8]) -> Vec<(DateTime<Utc>, CoordinatorStateSnapshot)> {
    let mut results = Vec::new();
    let mut offset = 0;
    while offset + COORD_RECORD_SIZE <= data.len() {
        let ts_bytes: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
        let timestamp_millis = i64::from_le_bytes(ts_bytes);
        let coord_bytes = &data[offset + 8..offset + COORD_RECORD_SIZE];
        let coord: &Coordinator = bytemuck::from_bytes(coord_bytes);
        let timestamp = DateTime::from_timestamp_millis(timestamp_millis).unwrap_or_default();
        let snapshot = coordinator_to_snapshot(timestamp, coord);
        results.push((timestamp, snapshot));
        offset += COORD_RECORD_SIZE;
    }
    results
}

fn coordinator_to_snapshot(
    timestamp: DateTime<Utc>,
    coord: &Coordinator,
) -> CoordinatorStateSnapshot {
    let checkpoint = match coord.model {
        psyche_coordinator::model::Model::LLM(llm) => llm.checkpoint,
    };
    let client_ids: Vec<String> = coord
        .epoch_state
        .clients
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    let round_clients_len = coord
        .current_round()
        .map(|r| r.clients_len as usize)
        .unwrap_or(0);
    let batch_assignments: BTreeMap<_, _> = if round_clients_len == coord.epoch_state.clients.len()
    {
        CommitteeSelection::from_coordinator(coord, 0)
            .map(|sel| {
                assign_data_for_state(coord, &sel)
                    .into_iter()
                    .map(|(batch_id, node_id)| (batch_id, node_id.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        tracing::debug!(
            round_clients_len,
            epoch_clients_len = coord.epoch_state.clients.len(),
            run_state = ?coord.run_state,
            "coordinator snapshot has mismatched client counts, skipping batch assignment"
        );
        BTreeMap::new()
    };
    CoordinatorStateSnapshot {
        timestamp,
        run_state: coord.run_state,
        epoch: coord.progress.epoch as u64,
        step: coord.progress.step as u64,
        checkpoint,
        client_ids,
        min_clients: coord.config.min_clients as usize,
        batch_assignments,
    }
}

pub enum TimelineEntry {
    Node {
        timestamp: DateTime<Utc>,
        node_id: String,
        event: Event,
    },
    Coordinator {
        timestamp: DateTime<Utc>,
        state: CoordinatorStateSnapshot,
    },
}

impl TimelineEntry {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            TimelineEntry::Node { timestamp, .. } => *timestamp,
            TimelineEntry::Coordinator { timestamp, .. } => *timestamp,
        }
    }

    pub fn node_id(&self) -> Option<&str> {
        match self {
            TimelineEntry::Node { node_id, .. } => Some(node_id),
            TimelineEntry::Coordinator { .. } => None,
        }
    }

    pub fn event_name(&self) -> String {
        match self {
            TimelineEntry::Coordinator { .. } => "coordinator update".to_string(),
            TimelineEntry::Node { event, .. } => event.data.to_string(),
        }
    }
}

/// Tracks the position (bytes consumed) in each .postcard file so `refresh()`
/// can incrementally read only new events.
struct LiveSource {
    dir: PathBuf,
    /// Maps each .postcard file → (bytes decoded, last mtime).
    file_positions: HashMap<PathBuf, (u64, SystemTime)>,
    /// Per-node total .postcard file bytes on disk, updated during refresh.
    node_file_sizes: HashMap<String, u64>,
}

/// Per-node first and last event timestamps.
#[derive(Debug, Clone)]
pub struct NodeTimestampRange {
    pub first: DateTime<Utc>,
    pub last: DateTime<Utc>,
}

pub struct ClusterTimeline {
    entries: Vec<TimelineEntry>,
    /// Snapshot materialized BEFORE applying the entry at `idx`.
    /// Stored every CHECKPOINT_INTERVAL entries for O(sqrt N) scrub.
    checkpoints: Vec<(usize, ClusterSnapshot)>,
    /// The projection state after applying ALL entries — used for incremental
    /// checkpoint building so we never replay the entire history.
    tail_projection: ClusterProjection,
    /// Present when the timeline was created from a directory; enables `refresh()`.
    live_source: Option<LiveSource>,
    /// Cached entity IDs in order of first appearance; invalidated on refresh.
    cached_entity_ids: Vec<String>,
    /// Set used to maintain `cached_entity_ids` incrementally.
    seen_entity_ids: HashSet<String>,
    /// Per-node timestamp range (first event, last event), maintained incrementally.
    node_timestamp_ranges: IndexMap<String, NodeTimestampRange>,
}

impl ClusterTimeline {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            checkpoints: Vec::new(),
            tail_projection: ClusterProjection::new(),
            live_source: None,
            cached_entity_ids: Vec::new(),
            seen_entity_ids: HashSet::new(),
            node_timestamp_ranges: IndexMap::new(),
        }
    }

    /// Scan all .postcard files under `dir` (recursing into node_id subdirs),
    /// decode events, sort by timestamp, and track file positions for live refresh.
    pub fn from_events_dir(dir: &Path) -> io::Result<Self> {
        Self::from_events_dir_with_progress(dir, |_| {})
    }

    /// Like [`from_events_dir`](Self::from_events_dir) but calls `progress` periodically
    /// so the caller can render a loading indicator.
    pub fn from_events_dir_with_progress(
        dir: &Path,
        mut progress: impl FnMut(&LoadProgress),
    ) -> io::Result<Self> {
        let mut p = LoadProgress {
            phase: "scanning",
            fraction: 0.0,
            total_bytes: 0,
            bytes_read: 0,
            entries: 0,
            files: 0,
        };

        // ── Phase 0: scan directory, count total bytes ───────────────────
        progress(&p);
        let mut all_files: Vec<(PathBuf, String, bool)> = Vec::new();
        let mut node_file_sizes: HashMap<String, u64> = HashMap::new();

        for dir_entry in std::fs::read_dir(dir)? {
            let dir_entry = dir_entry?;
            let node_dir = dir_entry.path();
            if !node_dir.is_dir() {
                continue;
            }
            let node_id = node_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let is_coord = node_id == COORDINATOR_SUBDIR;
            let accepted_ext = if is_coord { "bin" } else { "postcard" };

            let mut data_files: Vec<PathBuf> = std::fs::read_dir(&node_dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == accepted_ext))
                .collect();
            data_files.sort();

            for file_path in data_files {
                if let Ok(meta) = std::fs::metadata(&file_path) {
                    p.total_bytes += meta.len();
                    *node_file_sizes.entry(node_id.clone()).or_default() += meta.len();
                }
                p.files += 1;
                all_files.push((file_path, node_id.clone(), is_coord));
            }
        }
        progress(&p);

        // ── Phase 1: read & decode ───────────────────────────────────────
        p.phase = "reading";
        p.fraction = 0.0;
        let mut file_positions: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();
        let mut raw_entries: Vec<TimelineEntry> = Vec::new();

        for (file_path, node_id, is_coord) in &all_files {
            let data = std::fs::read(file_path)?;
            let file_len = data.len() as u64;
            let mtime = std::fs::metadata(file_path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let mut cursor = 0;

            if *is_coord {
                let records = decode_coordinator_records(&data);
                cursor = records.len() * COORD_RECORD_SIZE;
                for (timestamp, snapshot) in records {
                    raw_entries.push(TimelineEntry::Coordinator {
                        timestamp,
                        state: snapshot,
                    });
                }
            } else {
                while cursor < data.len() {
                    match try_decode_cobs_frame::<Event>(&data, &mut cursor) {
                        Some(event) => {
                            raw_entries.push(TimelineEntry::Node {
                                timestamp: event.timestamp,
                                node_id: node_id.clone(),
                                event,
                            });
                        }
                        None => break,
                    }
                }
            }

            file_positions.insert(file_path.clone(), (cursor as u64, mtime));
            p.bytes_read += file_len;
            p.entries = raw_entries.len();
            if p.total_bytes > 0 {
                p.fraction = p.bytes_read as f32 / p.total_bytes as f32;
            }
            progress(&p);
        }

        // ── Phase 2: sort ────────────────────────────────────────────────
        p.phase = "sorting";
        p.fraction = 0.0;
        progress(&p);
        raw_entries.sort_by_key(|e| e.timestamp());
        p.fraction = 1.0;
        progress(&p);

        // ── Phase 3: build checkpoints ───────────────────────────────────
        let entry_count = raw_entries.len();
        let mut timeline = Self {
            entries: raw_entries,
            checkpoints: Vec::new(),
            tail_projection: ClusterProjection::new(),
            live_source: Some(LiveSource {
                dir: dir.to_path_buf(),
                file_positions,
                node_file_sizes,
            }),
            cached_entity_ids: Vec::new(),
            seen_entity_ids: HashSet::new(),
            node_timestamp_ranges: IndexMap::new(),
        };
        timeline.rebuild_checkpoints_with_progress(entry_count, &mut p, &mut progress);
        timeline.rebuild_entity_ids();
        timeline.extend_timestamp_ranges(0);
        p.phase = "ready";
        p.fraction = 1.0;
        progress(&p);
        Ok(timeline)
    }

    /// Incrementally scan the source directory for new events appended since the
    /// last load or refresh. Returns `true` if any new events were found.
    ///
    /// Only callable when the timeline was created via `from_events_dir`.
    pub fn refresh(&mut self) -> io::Result<bool> {
        // Clone what we need to avoid holding a borrow while mutating self.
        let (dir, current_positions) = match &self.live_source {
            None => return Ok(false),
            Some(live) => (live.dir.clone(), live.file_positions.clone()),
        };

        let mut new_entries: Vec<TimelineEntry> = Vec::new();
        let mut updated_positions: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();
        let mut refreshed_file_sizes: HashMap<String, u64> = HashMap::new();

        // Walk node subdirectories — new nodes may have appeared since last scan.
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            return Ok(false);
        };

        for dir_entry in read_dir.filter_map(|e| e.ok()) {
            let node_dir = dir_entry.path();
            if !node_dir.is_dir() {
                continue;
            }
            let node_id = node_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let is_coord = node_id == COORDINATOR_SUBDIR;
            let accepted_ext = if is_coord { "bin" } else { "postcard" };

            let mut data_files: Vec<PathBuf> = std::fs::read_dir(&node_dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == accepted_ext))
                .collect();
            data_files.sort();

            for file_path in data_files {
                let Ok(meta) = std::fs::metadata(&file_path) else {
                    continue;
                };
                let file_mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

                // Accumulate per-node file sizes from the stat we already did.
                *refreshed_file_sizes.entry(node_id.clone()).or_default() += meta.len();

                // Skip files whose mtime hasn't changed — no new data to read.
                if let Some(&(_, cached_mtime)) = current_positions.get(&file_path) {
                    if file_mtime == cached_mtime {
                        continue;
                    }
                }

                let start_pos = current_positions
                    .get(&file_path)
                    .map(|&(pos, _)| pos)
                    .unwrap_or(0);

                // Read only the new bytes.
                let Ok(mut file) = std::fs::File::open(&file_path) else {
                    continue;
                };
                if file.seek(SeekFrom::Start(start_pos)).is_err() {
                    continue;
                }
                let mut new_data = Vec::new();
                if file.read_to_end(&mut new_data).is_err() {
                    continue;
                }

                let mut cursor = 0;
                if is_coord {
                    let records = decode_coordinator_records(&new_data);
                    cursor = records.len() * COORD_RECORD_SIZE;
                    for (timestamp, snapshot) in records {
                        new_entries.push(TimelineEntry::Coordinator {
                            timestamp,
                            state: snapshot,
                        });
                    }
                } else {
                    while cursor < new_data.len() {
                        match try_decode_cobs_frame::<Event>(&new_data, &mut cursor) {
                            Some(event) => {
                                new_entries.push(TimelineEntry::Node {
                                    timestamp: event.timestamp,
                                    node_id: node_id.clone(),
                                    event,
                                });
                            }
                            None => break,
                        }
                    }
                }

                updated_positions.insert(file_path, (start_pos + cursor as u64, file_mtime));
            }
        }

        // Apply position and size updates.
        if let Some(live) = &mut self.live_source {
            live.file_positions.extend(updated_positions);
            live.node_file_sizes = refreshed_file_sizes;
        }

        if new_entries.is_empty() {
            return Ok(false);
        }

        // Sort new entries, then merge into the existing (already-sorted) entries.
        new_entries.sort_by_key(|e| e.timestamp());

        let newest_existing = self
            .entries
            .last()
            .map(|e| e.timestamp())
            .unwrap_or_default();

        if new_entries
            .first()
            .map(|e| e.timestamp())
            .unwrap_or_default()
            < newest_existing
        {
            // New events are older than the newest existing event — they need to
            // be merged into the sorted timeline. Find the point where new events
            // start interleaving, roll back to the last checkpoint before that,
            // merge, and re-index from there.
            let earliest_new_ts = new_entries.first().unwrap().timestamp();
            let merge_point = self
                .entries
                .partition_point(|e| e.timestamp() < earliest_new_ts);

            // Keep only checkpoints before the merge point.
            let truncate_to = self
                .checkpoints
                .iter()
                .position(|(ci, _)| *ci >= merge_point)
                .unwrap_or(self.checkpoints.len());
            self.checkpoints.truncate(truncate_to);

            // Pop the last valid checkpoint to use as our replay base.
            let (replay_start, base_snapshot) = self
                .checkpoints
                .pop()
                .unwrap_or((0, ClusterSnapshot::new()));
            self.tail_projection = ClusterProjection::from_snapshot(base_snapshot);

            // Merge new entries and re-sort.
            self.entries.extend(new_entries);
            self.entries.sort_by_key(|e| e.timestamp());

            // Rebuild checkpoints and projection from the replay point.
            self.extend_checkpoints(replay_start);

            // Rebuild entity ID and timestamp caches from scratch since
            // insertion order may have changed.
            self.rebuild_entity_ids();
            self.node_timestamp_ranges.clear();
            self.extend_timestamp_ranges(0);
        } else {
            // Fast path: new events are all newer, just append.
            let append_start = self.entries.len();
            self.entries.extend(new_entries);
            self.extend_checkpoints(append_start);
            self.extend_entity_ids(append_start);
            self.extend_timestamp_ranges(append_start);
        }
        Ok(true)
    }

    pub fn push_node_event(&mut self, node_id: String, event: Event) {
        let timestamp = event.timestamp;
        let append_start = self.entries.len();
        self.entries.push(TimelineEntry::Node {
            timestamp,
            node_id: node_id.clone(),
            event,
        });
        self.extend_checkpoints(append_start);
        if self.seen_entity_ids.insert(node_id.clone()) {
            self.cached_entity_ids.push(node_id);
        }
        self.extend_timestamp_ranges(append_start);
    }

    pub fn push_coordinator(&mut self, state: CoordinatorStateSnapshot) {
        let timestamp = state.timestamp;
        let append_start = self.entries.len();
        self.entries
            .push(TimelineEntry::Coordinator { timestamp, state });
        self.extend_checkpoints(append_start);
        // "coordinator" is always present; update entity cache.
        if self.seen_entity_ids.insert("coordinator".to_string()) {
            self.cached_entity_ids.push("coordinator".to_string());
        }
    }

    /// Rebuild the materialized checkpoint index from scratch.
    /// Only used on initial load.
    fn rebuild_checkpoints_with_progress(
        &mut self,
        total_hint: usize,
        p: &mut LoadProgress,
        progress: &mut impl FnMut(&LoadProgress),
    ) {
        self.checkpoints.clear();
        self.tail_projection = ClusterProjection::new();
        let total = if total_hint > 0 {
            total_hint
        } else {
            self.entries.len()
        };
        let report_every = (total / 100).max(1);

        p.phase = "indexing";
        p.fraction = 0.0;
        progress(p);

        for (idx, entry) in self.entries.iter().enumerate() {
            if idx % CHECKPOINT_INTERVAL == 0 {
                self.checkpoints
                    .push((idx, self.tail_projection.snapshot().clone()));
            }
            Self::apply_entry(&mut self.tail_projection, entry);
            if total > 0 && idx % report_every == 0 {
                p.fraction = idx as f32 / total as f32;
                progress(p);
            }
        }
    }

    /// Incrementally extend checkpoints for entries appended at `start_idx..`.
    /// Reuses `tail_projection` which already has all entries before `start_idx` applied.
    fn extend_checkpoints(&mut self, start_idx: usize) {
        for idx in start_idx..self.entries.len() {
            if idx % CHECKPOINT_INTERVAL == 0 {
                self.checkpoints
                    .push((idx, self.tail_projection.snapshot().clone()));
            }
            Self::apply_entry(&mut self.tail_projection, &self.entries[idx]);
        }
    }

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

    /// Rebuild the entity ID cache from scratch (used on initial load).
    fn rebuild_entity_ids(&mut self) {
        self.seen_entity_ids.clear();
        self.cached_entity_ids.clear();
        self.extend_entity_ids(0);
    }

    /// Incrementally update per-node timestamp ranges for entries at `start_idx..`.
    fn extend_timestamp_ranges(&mut self, start_idx: usize) {
        for entry in &self.entries[start_idx..] {
            let (id, ts) = match entry {
                TimelineEntry::Node {
                    node_id, timestamp, ..
                } => (node_id.clone(), *timestamp),
                TimelineEntry::Coordinator { .. } => continue,
            };
            self.node_timestamp_ranges
                .entry(id)
                .and_modify(|r| {
                    if ts < r.first {
                        r.first = ts;
                    }
                    if ts > r.last {
                        r.last = ts;
                    }
                })
                .or_insert(NodeTimestampRange {
                    first: ts,
                    last: ts,
                });
        }
    }

    /// Per-node first/last event timestamps.
    pub fn node_timestamp_ranges(&self) -> &IndexMap<String, NodeTimestampRange> {
        &self.node_timestamp_ranges
    }

    /// Incrementally update entity ID cache for entries appended at `start_idx..`.
    fn extend_entity_ids(&mut self, start_idx: usize) {
        for entry in &self.entries[start_idx..] {
            let id = match entry {
                TimelineEntry::Node { node_id, .. } => node_id.clone(),
                TimelineEntry::Coordinator { .. } => "coordinator".to_string(),
            };
            if self.seen_entity_ids.insert(id.clone()) {
                self.cached_entity_ids.push(id);
            }
        }
    }

    /// Replay entries from the nearest materialized checkpoint to produce a
    /// ClusterSnapshot at position `idx`. O(sqrt N) amortized.
    pub fn snapshot_at(&self, idx: usize) -> ClusterSnapshot {
        if self.entries.is_empty() {
            return ClusterSnapshot::new();
        }
        let idx = idx.min(self.entries.len() - 1);

        let (start_idx, base_snapshot) = self
            .checkpoints
            .iter()
            .rev()
            .find(|(ci, _)| *ci <= idx)
            .map(|(ci, snap)| (*ci, snap.clone()))
            .unwrap_or_else(|| (0, ClusterSnapshot::new()));

        let mut proj = ClusterProjection::from_snapshot(base_snapshot);

        for i in start_idx..=idx {
            match &self.entries[i] {
                TimelineEntry::Node { node_id, event, .. } => {
                    proj.apply_node_event(node_id, event);
                }
                TimelineEntry::Coordinator { state, .. } => {
                    proj.apply_coordinator(state.clone());
                }
            }
        }

        proj.into_snapshot()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[TimelineEntry] {
        &self.entries
    }

    /// All entity IDs that appear anywhere in the timeline, in order of first appearance.
    /// Returns a reference to the cached list — O(1).
    pub fn all_entity_ids(&self) -> &[String] {
        &self.cached_entity_ids
    }

    pub fn timestamp_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let first = self.entries.first()?.timestamp();
        let last = self.entries.last()?.timestamp();
        Some((first, last))
    }

    /// Per-node total .postcard bytes on disk, updated during `refresh()`.
    /// Returns an empty map when the timeline has no live source.
    pub fn node_file_sizes(&self) -> &HashMap<String, u64> {
        static EMPTY: std::sync::LazyLock<HashMap<String, u64>> =
            std::sync::LazyLock::new(HashMap::new);
        match &self.live_source {
            Some(live) => &live.node_file_sizes,
            None => &EMPTY,
        }
    }
}

impl Default for ClusterTimeline {
    fn default() -> Self {
        Self::new()
    }
}
