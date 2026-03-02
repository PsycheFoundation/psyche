use psyche_event_sourcing::{
    events::{Client, EventData},
    timeline::{ClusterTimeline, TimelineEntry},
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use std::collections::{BTreeMap, HashSet};
use strum::EnumIter;

use crate::utils::short_id;

/// Event category used for filtering, coloring, and visual priority.
///
/// Variant order encodes display priority — earlier variants win when multiple
/// events share the same waterfall slot. `Other` and `P2P` are lowest priority
/// so they don't obscure training or error events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, EnumIter)]
pub enum EventCategory {
    Error,
    Train,
    Warmup,
    Cooldown,
    Client,
    Coordinator,
    Other,
    P2P,
}

impl EventCategory {
    pub fn color(self) -> Color {
        match self {
            Self::Error => Color::Red,
            Self::Train => Color::Green,
            Self::Warmup => Color::Magenta,
            Self::Cooldown => Color::Cyan,
            Self::P2P => Color::Blue,
            Self::Client => Color::Yellow,
            Self::Coordinator => Color::White,
            Self::Other => Color::DarkGray,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Train => "train",
            Self::Warmup => "warmup",
            Self::Cooldown => "cool",
            Self::P2P => "p2p",
            Self::Client => "client",
            Self::Coordinator => "coord",
            Self::Other => "other",
        }
    }

    pub fn key(self) -> char {
        match self {
            Self::Error => 'e',
            Self::Train => 't',
            Self::Warmup => 'w',
            Self::Cooldown => 'c',
            Self::P2P => 'p',
            Self::Client => 'l',      // cLient
            Self::Coordinator => 'o', // cOord
            Self::Other => '?',
        }
    }

    /// Whether this category should appear in the filter picker bar.
    /// `Other` is hidden since it's a catch-all.
    pub fn filterable(self) -> bool {
        self != Self::Other
    }

    /// Classify an `EventData` into a category.
    pub fn classify(data: &EventData) -> Self {
        match data {
            EventData::Client(Client::Error(_) | Client::Warning(_)) => Self::Error,
            EventData::Train(_) => Self::Train,
            EventData::Warmup(_) => Self::Warmup,
            EventData::Cooldown(_) => Self::Cooldown,
            EventData::P2P(_) => Self::P2P,
            EventData::Client(_) => Self::Client,
            _ => Self::Other,
        }
    }
}

/// Returns true if a timeline entry passes the given category filter set.
/// An empty filter means "show everything".
pub fn entry_matches_filter(entry: &TimelineEntry, filter: &HashSet<EventCategory>) -> bool {
    if filter.is_empty() {
        return true;
    }
    match entry {
        TimelineEntry::Node { event, .. } => {
            let cat = EventCategory::classify(&event.data);
            filter.contains(&cat)
        }
        TimelineEntry::Coordinator { .. } => filter.contains(&EventCategory::Coordinator),
    }
}

/// Width of the node name column on the left side of the waterfall.
const NODE_NAME_W: u16 = 16;

/// Horizontal event-track view with a fixed-size zoom window.
///
/// Shows `zoom` timeline entries at a time. `x_scroll` is the index of the
/// first visible entry. The scrubber `cursor` entry is always highlighted.
/// Node names are shown on the left; rows scroll vertically with `node_scroll`.
pub struct WaterfallWidget<'a> {
    pub timeline: &'a ClusterTimeline,
    pub cursor: usize,
    /// All entity IDs in the timeline, in order of first appearance.
    pub node_ids: &'a [String],
    pub selected_node_idx: Option<usize>,
    pub node_scroll: usize,
    /// How many timeline entries fit in the visible window.
    pub zoom: usize,
    /// Index of the first timeline entry in the visible window.
    pub x_scroll: usize,
    /// Set of active category filters. Events match if their category is in this set.
    /// Empty set means show all events.
    pub filter: &'a HashSet<EventCategory>,
}

impl<'a> Widget for WaterfallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let total = self.timeline.len();
        if total == 0 {
            return;
        }

        // Split area: node name column (left) | track area (right).
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(NODE_NAME_W), Constraint::Min(5)])
            .split(area);
        let name_area = cols[0];
        let track_area = cols[1];

        let zoom = self.zoom.max(1);
        let win_start = self.x_scroll.min(total.saturating_sub(1));
        let win_end = (win_start + zoom).min(total);
        let win_size = (win_end - win_start).max(1);
        let track_w = track_area.width as usize;

        // Column range for slot `s` (0-based within window).
        let slot_col_start = |s: usize| s * track_w / win_size;

        // Which slot (if any) holds the cursor.
        let cursor_slot: Option<usize> = if self.cursor >= win_start && self.cursor < win_end {
            Some(self.cursor - win_start)
        } else {
            None
        };

        // Build per-entity slot map: entity_id → slot_index → (category, display_name).
        let mut node_events: BTreeMap<&str, BTreeMap<usize, (EventCategory, String)>> =
            BTreeMap::new();
        for (slot, entry) in self.timeline.entries()[win_start..win_end]
            .iter()
            .enumerate()
        {
            match entry {
                TimelineEntry::Node { node_id, event, .. } => {
                    let cat = EventCategory::classify(&event.data);
                    if !self.filter.is_empty() && !self.filter.contains(&cat) {
                        continue;
                    }
                    let name = event.data.to_string();
                    let slot_entry = node_events
                        .entry(node_id.as_str())
                        .or_default()
                        .entry(slot)
                        .or_insert((cat, name.clone()));
                    if cat < slot_entry.0 {
                        *slot_entry = (cat, name);
                    }
                }
                TimelineEntry::Coordinator { state, .. } => {
                    if !self.filter.is_empty() && !self.filter.contains(&EventCategory::Coordinator)
                    {
                        continue;
                    }
                    let name = format!("{}", state.run_state);
                    node_events
                        .entry("coordinator")
                        .or_default()
                        .insert(slot, (EventCategory::Coordinator, name));
                }
            }
        }

        let entries = self.timeline.entries();

        // ── Row 0 (track area): timestamp ruler ──────────────────────────────
        let ruler_y = track_area.y;
        // Also paint the name column ruler row.
        let name_ruler: String = "─".repeat(name_area.width as usize);
        buf.set_string(
            name_area.x,
            ruler_y,
            &name_ruler,
            Style::default().fg(Color::DarkGray),
        );

        let dashes: String = "─".repeat(track_w);
        buf.set_string(
            track_area.x,
            ruler_y,
            &dashes,
            Style::default().fg(Color::DarkGray),
        );

        // Find the training step at each slot in the visible window.
        let step_at_slot: Vec<Option<u64>> = {
            let mut cur: Option<u64> = None;
            for i in (0..win_start).rev() {
                if let TimelineEntry::Coordinator { state, .. } = &entries[i] {
                    cur = Some(state.step);
                    break;
                }
            }
            let mut out = vec![None; win_size];
            for i in win_start..win_end {
                if let TimelineEntry::Coordinator { state, .. } = &entries[i] {
                    cur = Some(state.step);
                }
                out[i - win_start] = cur;
            }
            out
        };

        // Write step labels on the ruler.
        (0..win_size)
            .filter_map(|slot| {
                let c_start = slot_col_start(slot);
                entries.get(win_start + slot)?;
                let label = match step_at_slot[slot] {
                    Some(step) => format!("step {step}"),
                    None => format!("{}", win_start + slot),
                };
                Some((c_start, label))
            })
            .scan(None::<String>, |prev, (c_start, label)| {
                if prev.as_deref() == Some(label.as_str()) {
                    Some(None)
                } else {
                    *prev = Some(label.clone());
                    Some(Some((c_start, label)))
                }
            })
            .flatten()
            .take_while(|(c_start, label)| c_start + label.len() <= track_w)
            .scan(0usize, |last_end, (c_start, label)| {
                if c_start < *last_end {
                    Some(None)
                } else {
                    *last_end = c_start + label.len() + 4;
                    Some(Some((c_start, label)))
                }
            })
            .flatten()
            .for_each(|(c_start, label)| {
                buf.set_string(
                    track_area.x + c_start as u16,
                    ruler_y,
                    &label,
                    Style::default().fg(Color::White),
                );
            });

        let zoom_label = format!(" zoom:{zoom:2} ");
        if zoom_label.len() <= track_w {
            buf.set_string(
                track_area.x + (track_w - zoom_label.len()) as u16,
                ruler_y,
                &zoom_label,
                Style::default().fg(Color::White),
            );
        }

        // ── Scroll indicators and node rows ──────────────────────────────────
        // Virtual row list: row 0 = "all info", rows 1..=N = actual nodes.
        // node_scroll is an offset into this virtual list.
        let body_h = area.height.saturating_sub(1) as usize; // rows below the ruler
        if body_h < 2 {
            return;
        }

        // Total virtual rows: 1 ("all info") + node count.
        let total_virtual = 1 + self.node_ids.len();

        // Top scroll indicator (row 1 below ruler).
        let top_indicator_y = area.y + 1;
        let rows_above = self.node_scroll;
        if rows_above > 0 {
            let label = format!("  ↑ {} more", rows_above);
            let label: String = label.chars().take(name_area.width as usize).collect();
            buf.set_string(
                name_area.x,
                top_indicator_y,
                format!("{:<w$}", label, w = name_area.width as usize),
                Style::default().fg(Color::DarkGray),
            );
        }

        // Node rows start at row 2 (after ruler + top indicator).
        let node_rows_start_y = area.y + 2;
        // Reserve 1 row at bottom for bottom scroll indicator.
        let max_visible_rows = body_h.saturating_sub(2);

        for row in 0..max_visible_rows {
            let virt_row = self.node_scroll + row;
            if virt_row >= total_virtual {
                break;
            }
            let y = node_rows_start_y + row as u16;
            if y >= area.y + area.height.saturating_sub(1) {
                break; // leave room for bottom indicator
            }

            if virt_row == 0 {
                // ── "all info" row ────────────────────────────────────────────
                let is_selected = self.selected_node_idx.is_none();
                let max_name = (name_area.width as usize).saturating_sub(2);
                let prefix = if is_selected { "► " } else { "  " };
                let label = "all info";
                let name_label = format!("{}{:<w$}", prefix, label, w = max_name);
                let name_label: String =
                    name_label.chars().take(name_area.width as usize).collect();
                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(name_area.x, y, &name_label, name_style);

                // Track: show all events merged across all entities.
                let dots: String = "-".repeat(track_w);
                buf.set_string(track_area.x, y, &dots, {
                    if is_selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                });

                // Merge all node_events into a single slot map (highest priority wins).
                let mut merged: BTreeMap<usize, (EventCategory, &str)> = BTreeMap::new();
                for slot_map in node_events.values() {
                    for (&slot, (cat, name)) in slot_map.iter() {
                        let entry = merged.entry(slot).or_insert((*cat, name.as_str()));
                        if *cat < entry.0 {
                            *entry = (*cat, name.as_str());
                        }
                    }
                }
                for (&slot, &(cat, name)) in &merged {
                    let stop_col = (slot + 1..win_size)
                        .find(|&s| merged.contains_key(&s))
                        .map(&slot_col_start)
                        .unwrap_or(track_w);
                    let available = stop_col - slot_col_start(slot);
                    let text: String = "▶".chars().chain(name.chars()).take(available).collect();
                    let style = if is_selected {
                        Style::default()
                            .fg(cat.color())
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(cat.color())
                    };
                    buf.set_string(track_area.x + slot_col_start(slot) as u16, y, &text, style);
                }

                // Cursor indicator on "all info" row.
                if is_selected && let Some(cs) = cursor_slot {
                    let c_start = slot_col_start(cs);
                    buf.set_string(
                        track_area.x + c_start as u16,
                        y,
                        "│",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            } else {
                // ── Regular node row ──────────────────────────────────────────
                let node_idx = virt_row - 1;
                let Some(node_id) = self.node_ids.get(node_idx) else {
                    break;
                };
                let is_selected = self.selected_node_idx == Some(node_idx);

                // Node name (left column).
                let max_name = (name_area.width as usize).saturating_sub(2);
                let node_short = short_id(node_id, max_name);
                let prefix = if is_selected { "► " } else { "  " };
                let name_label = format!("{}{:<w$}", prefix, node_short, w = max_name);
                let name_label: String =
                    name_label.chars().take(name_area.width as usize).collect();
                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(name_area.x, y, &name_label, name_style);

                // Event track (right column).
                let empty = BTreeMap::new();
                let slot_map = node_events.get(node_id.as_str()).unwrap_or(&empty);

                let dots: String = "-".repeat(track_w);
                buf.set_string(track_area.x, y, &dots, {
                    if is_selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                });

                for slot in 0..win_size {
                    let Some((cat, name)) = slot_map.get(&slot) else {
                        continue;
                    };
                    let stop_col = (slot + 1..win_size)
                        .find(|&s| slot_map.contains_key(&s))
                        .map(&slot_col_start)
                        .unwrap_or(track_w);

                    let available = stop_col - slot_col_start(slot);
                    let text: String = "▶".chars().chain(name.chars()).take(available).collect();
                    let style = if is_selected {
                        Style::default()
                            .fg(cat.color())
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(cat.color())
                    };
                    buf.set_string(track_area.x + slot_col_start(slot) as u16, y, &text, style);
                }

                // Cursor indicator.
                let cursor_on_this_row = self
                    .selected_node_idx
                    .map(|sel| sel == node_idx)
                    .unwrap_or(false);
                if cursor_on_this_row && let Some(cs) = cursor_slot {
                    let c_start = slot_col_start(cs);
                    buf.set_string(
                        track_area.x + c_start as u16,
                        y,
                        "│",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            }
        }

        // Bottom scroll indicator.
        let visible_last = self.node_scroll + max_visible_rows;
        let rows_below = total_virtual.saturating_sub(visible_last);
        let bottom_y = area.y + area.height.saturating_sub(1);
        if rows_below > 0 {
            let label = format!("  ↓ {} more", rows_below);
            let label: String = label.chars().take(name_area.width as usize).collect();
            buf.set_string(
                name_area.x,
                bottom_y,
                format!("{:<w$}", label, w = name_area.width as usize),
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}
