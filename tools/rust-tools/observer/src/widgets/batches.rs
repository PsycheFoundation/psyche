use std::collections::BTreeMap;

use psyche_core::BatchId;
use psyche_event_sourcing::projection::{ClusterBatchView, ClusterSnapshot, NodeBatchStatus};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::utils::short_id;

pub struct BatchesWidget<'a> {
    pub snapshot: &'a ClusterSnapshot,
    /// Highlighted node — its rows are shown in a distinct colour, and the
    /// widget auto-scrolls to keep this node's assigned batch visible.
    pub selected_node_id: Option<&'a str>,
}

fn fmt_batch_id(batch_id: &BatchId) -> String {
    let s = batch_id.0.start;
    let e = batch_id.0.end;
    if s == e {
        format!("{s}")
    } else {
        format!("{s}..{e}")
    }
}

/// Build a sorted list of all node IDs across the snapshot.
fn all_node_ids(snapshot: &ClusterSnapshot) -> Vec<String> {
    let mut ids: Vec<String> = snapshot.nodes.keys().cloned().collect();
    ids.sort();
    ids
}

// ── status characters ─────────────────────────────────────────────────────────

fn status_char(status: &NodeBatchStatus) -> (char, Color) {
    if status.deserialized == Some(false) || status.download == Some(Some(false)) {
        return ('✗', Color::Red);
    }
    if status.deserialized == Some(true) {
        return ('●', Color::Green);
    }
    if status.download == Some(Some(true)) {
        return ('◉', Color::Cyan);
    }
    if status.download == Some(None) {
        return ('↓', Color::Cyan);
    }
    if status.gossip_received {
        return ('○', Color::Yellow);
    }
    ('·', Color::DarkGray)
}

fn check_char(ok: Option<bool>) -> (char, Color) {
    match ok {
        None => ('—', Color::DarkGray),
        Some(true) => ('✓', Color::Green),
        Some(false) => ('✗', Color::Red),
    }
}

// ── per-row dot rendering ────────────────────────────────────────────────────

/// Each dot: (char, color, is_selected_node).
fn node_dot_lines(
    batch: &ClusterBatchView,
    all_nodes: &[String],
    selected_node_id: Option<&str>,
    wrap_w: usize,
) -> Vec<Vec<(char, Color, bool)>> {
    let trainer = batch.assigned_to.as_deref();
    let mut chars: Vec<(char, Color, bool)> = Vec::new();

    for node_id in all_nodes {
        if trainer == Some(node_id.as_str()) {
            continue;
        }
        let is_sel = selected_node_id == Some(node_id.as_str());
        let status = batch.node_status.get(node_id.as_str());
        let (ch, color) = match status {
            Some(s) => status_char(s),
            None => ('·', Color::DarkGray),
        };
        chars.push((ch, color, is_sel));
    }

    if chars.is_empty() || wrap_w == 0 {
        return vec![chars];
    }
    chars.chunks(wrap_w).map(|c| c.to_vec()).collect()
}

fn applied_dot_lines(
    batch: &ClusterBatchView,
    all_nodes: &[String],
    applied_by: &std::collections::HashSet<String>,
    selected_node_id: Option<&str>,
    wrap_w: usize,
) -> Vec<Vec<(char, Color, bool)>> {
    let trainer = batch.assigned_to.as_deref();
    let mut chars: Vec<(char, Color, bool)> = Vec::new();

    for node_id in all_nodes {
        if trainer == Some(node_id.as_str()) {
            continue;
        }
        let is_sel = selected_node_id == Some(node_id.as_str());
        let (ch, color) = if applied_by.contains(node_id.as_str()) {
            ('✓', Color::Green)
        } else {
            ('·', Color::DarkGray)
        };
        chars.push((ch, color, is_sel));
    }

    if chars.is_empty() || wrap_w == 0 {
        return vec![chars];
    }
    chars.chunks(wrap_w).map(|c| c.to_vec()).collect()
}

// ── column layout ────────────────────────────────────────────────────────────

const COL_BATCH_ID: u16 = 16;
const COL_NODE: u16 = 15;
const COL_DATA: u16 = 5;
const COL_TRAIN: u16 = 6;
/// Total fixed prefix width including leading space and inter-column spaces.
const PREFIX_W: u16 = 1 + COL_BATCH_ID + 1 + COL_NODE + 1 + COL_DATA + COL_TRAIN;

/// Compute the widths for the NODES and APPLIED dot columns.
/// Each column is sized to fit its content (header text or dot count),
/// but won't exceed the available space.
fn column_layout(total_w: u16, node_count: usize) -> (u16, u16) {
    let remaining = total_w.saturating_sub(PREFIX_W);
    if remaining <= 1 {
        return (0, 0);
    }
    // Available for both columns plus the 1-char separator between them.
    let available = remaining - 1;
    // Minimum width: enough for the header text.
    // "NODES" = 5, "APPLIED" = 7. Dot count is the actual content width.
    let nodes_need = (node_count as u16).max(5);
    let applied_need = (node_count as u16).max(7);
    // Each column gets what it needs, but capped at half the available space.
    let half = available / 2;
    let nodes_w = nodes_need.min(half).min(available);
    let applied_w = applied_need
        .min(half)
        .min(available.saturating_sub(nodes_w));
    (nodes_w, applied_w)
}

// ── section rendering ─────────────────────────────────────────────────────────

struct SectionCtx<'a> {
    all_nodes: &'a [String],
    selected_node_id: Option<&'a str>,
    applied_by: &'a std::collections::HashSet<String>,
}

/// Render the fixed header row directly into the buffer using ratatui Layout
/// for proper centering.
fn render_section_header(area: Rect, buf: &mut Buffer, nodes_w: u16, applied_w: u16) {
    if area.height == 0 || area.width < PREFIX_W {
        return;
    }

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    // Split the header row into columns using Layout.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),            // leading space
            Constraint::Length(COL_BATCH_ID), // BATCH ID
            Constraint::Length(1),            // space
            Constraint::Length(COL_NODE),     // NODE
            Constraint::Length(1),            // space
            Constraint::Length(COL_DATA),     // DATA
            Constraint::Length(COL_TRAIN),    // TRAIN
            Constraint::Length(nodes_w),      // NODES dots
            Constraint::Length(1),            // separator
            Constraint::Length(applied_w),    // APPLIED dots
        ])
        .split(area);

    // Render each column header centered in its allocated area.
    Paragraph::new("BATCH ID")
        .style(header_style)
        .render(cols[1], buf);
    Paragraph::new("NODE")
        .style(header_style)
        .render(cols[3], buf);
    Paragraph::new("DATA")
        .style(header_style)
        .render(cols[5], buf);
    Paragraph::new("TRAIN")
        .style(header_style)
        .render(cols[6], buf);

    if nodes_w > 0 {
        Paragraph::new("NODES")
            .style(header_style)
            .alignment(Alignment::Center)
            .render(cols[7], buf);
    }
    if applied_w > 0 {
        Paragraph::new("APPLIED")
            .style(header_style)
            .alignment(Alignment::Center)
            .render(cols[9], buf);
    }
}

/// A tagged line: either a normal data line, or one that belongs to the selected
/// node's assigned batch (used for auto-scroll targeting).
struct TaggedLine<'a> {
    line: Line<'a>,
    /// True if this line belongs to a batch assigned to the selected node.
    is_selected_batch: bool,
}

/// Build data lines for one batch-table section (prev or current step).
/// Returns tagged lines so the caller can compute auto-scroll.
fn build_section_data_lines<'a>(
    batches: &BTreeMap<BatchId, ClusterBatchView>,
    ctx: &SectionCtx<'a>,
    nodes_w: u16,
    applied_w: u16,
) -> Vec<TaggedLine<'a>> {
    let mut lines: Vec<TaggedLine> = Vec::new();
    let nodes_w = nodes_w as usize;
    let applied_w = applied_w as usize;

    for (batch_id, batch) in batches.iter() {
        let is_mine = ctx
            .selected_node_id
            .is_some_and(|sel| batch.assigned_to.as_deref() == Some(sel));
        let row_style = if is_mine {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let bid = fmt_batch_id(batch_id);
        let assigned = batch
            .assigned_to
            .as_deref()
            .map(|id| short_id(id, COL_NODE as usize))
            .unwrap_or_else(|| "—".to_string());

        let (data_ch, data_color) = check_char(batch.data_downloaded);
        let (train_ch, train_color) = if batch.trained {
            ('✓', Color::Green)
        } else {
            ('—', Color::DarkGray)
        };

        let node_wrap = if nodes_w > 0 { nodes_w } else { 1 };
        let app_wrap = if applied_w > 0 { applied_w } else { 1 };
        let node_lines = node_dot_lines(batch, ctx.all_nodes, ctx.selected_node_id, node_wrap);
        let app_lines = applied_dot_lines(
            batch,
            ctx.all_nodes,
            ctx.applied_by,
            ctx.selected_node_id,
            app_wrap,
        );
        let num_rows = node_lines.len().max(app_lines.len()).max(1);

        for row_i in 0..num_rows {
            let mut spans: Vec<Span> = Vec::new();

            if row_i == 0 {
                spans.push(Span::styled(
                    format!(" {:<w$} ", bid, w = COL_BATCH_ID as usize),
                    row_style,
                ));
                spans.push(Span::styled(
                    format!("{:<w$} ", assigned, w = COL_NODE as usize),
                    row_style,
                ));
                spans.push(Span::styled(
                    format!(" {data_ch}   "),
                    Style::default().fg(data_color),
                ));
                spans.push(Span::styled(
                    format!(" {train_ch}   "),
                    Style::default().fg(train_color),
                ));
            } else {
                spans.push(Span::raw(" ".repeat(PREFIX_W as usize)));
            }

            // Node dots — highlight the selected node's dot.
            if let Some(dots) = node_lines.get(row_i) {
                for &(ch, color, is_sel) in dots {
                    let style = if is_sel {
                        Style::default()
                            .fg(Color::Black)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    spans.push(Span::styled(ch.to_string(), style));
                }
                let used = dots.len();
                if used < nodes_w {
                    spans.push(Span::raw(" ".repeat(nodes_w - used)));
                }
            } else {
                spans.push(Span::raw(" ".repeat(nodes_w)));
            }

            spans.push(Span::raw(" "));

            // Applied dots — highlight the selected node's dot.
            if let Some(dots) = app_lines.get(row_i) {
                for &(ch, color, is_sel) in dots {
                    let style = if is_sel {
                        Style::default()
                            .fg(Color::Black)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    spans.push(Span::styled(ch.to_string(), style));
                }
            }

            lines.push(TaggedLine {
                line: Line::from(spans),
                is_selected_batch: is_mine,
            });
        }
    }

    lines
}

// ── witness rendering ────────────────────────────────────────────────────────

/// Witness progress state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WitnessState {
    /// Elected but RPC not yet submitted.
    Waiting,
    /// RPC submitted, awaiting result.
    Sending,
    /// RPC returned success.
    Sent,
    /// Coordinator step has advanced past witness step (quorum met).
    Confirmed,
}

fn witness_state(
    ws: &psyche_event_sourcing::projection::WitnessStatus,
    coordinator_step: u64,
) -> WitnessState {
    // If the coordinator has moved past this witness's step, it's confirmed.
    if coordinator_step > ws.info.step {
        return WitnessState::Confirmed;
    }
    match (ws.submitted, ws.rpc_result) {
        (_, Some(true)) => WitnessState::Sent,
        (true, None) => WitnessState::Sending,
        _ => WitnessState::Waiting,
    }
}

fn build_witness_lines<'a>(
    snapshot: &'a ClusterSnapshot,
    selected_node_id: Option<&str>,
) -> Vec<Line<'a>> {
    if snapshot.step_witnesses.is_empty() {
        return Vec::new();
    }

    let coord_step = snapshot.coordinator.as_ref().map(|c| c.step).unwrap_or(0);

    let witnesses: Vec<_> = snapshot.step_witnesses.iter().collect();

    if witnesses.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<Line> = Vec::new();
    let step = snapshot.coordinator.as_ref().map(|c| c.step).unwrap_or(0);
    let label = format!("Witnesses (step {})", step);

    lines.push(Line::from("")); // blank separator
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(label, Style::default().fg(Color::Magenta).bold()),
    ]));

    // Progress stages: waiting → sending → sent → confirmed
    let stages = ["waiting", "sending", "sent", "confirmed"];

    for (node_id, ws) in &witnesses {
        let id = short_id(node_id, COL_NODE as usize);
        let is_selected = selected_node_id == Some(node_id.as_str());
        let state = witness_state(ws, coord_step);
        let active_idx = match state {
            WitnessState::Waiting => 0,
            WitnessState::Sending => 1,
            WitnessState::Sent => 2,
            WitnessState::Confirmed => 3,
        };

        let id_style = if is_selected {
            Style::default().fg(Color::Yellow).bold()
        } else {
            Style::default()
        };
        let mut spans: Vec<Span> = vec![Span::styled(format!("  {:<15} ", id), id_style)];

        // Render each stage as a lit/dim indicator.
        for (i, &stage_label) in stages.iter().enumerate() {
            let (indicator, color) = if i <= active_idx {
                (
                    "●",
                    match i {
                        0 => Color::Yellow,
                        1 => Color::Cyan,
                        2 => Color::Blue,
                        3 => Color::Green,
                        _ => Color::White,
                    },
                )
            } else {
                ("○", Color::DarkGray)
            };
            spans.push(Span::styled(
                indicator.to_string(),
                Style::default().fg(color),
            ));
            spans.push(Span::styled(
                format!(" {stage_label} "),
                if i <= active_idx {
                    Style::default().fg(color)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ));
        }

        // Show failure if RPC rejected.
        if ws.rpc_result == Some(false) {
            spans.push(Span::styled(" ✗ rejected", Style::default().fg(Color::Red)));
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn build_legend_lines<'a>() -> Vec<Line<'a>> {
    let dim = Style::default().fg(Color::DarkGray);
    vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Legend", Style::default().fg(Color::DarkGray).bold()),
        ]),
        Line::from(vec![
            Span::styled(" NODES: ", dim.add_modifier(Modifier::BOLD)),
            Span::styled("·", Style::default().fg(Color::DarkGray)),
            Span::styled(" waiting ", dim),
            Span::styled("○", Style::default().fg(Color::Yellow)),
            Span::styled(" gossip ", dim),
            Span::styled("↓", Style::default().fg(Color::Cyan)),
            Span::styled(" downloading ", dim),
            Span::styled("◉", Style::default().fg(Color::Cyan)),
            Span::styled(" downloaded ", dim),
            Span::styled("●", Style::default().fg(Color::Green)),
            Span::styled(" ready ", dim),
            Span::styled("✗", Style::default().fg(Color::Red)),
            Span::styled(" failed", dim),
        ]),
        Line::from(vec![
            Span::styled(" APPLIED: ", dim.add_modifier(Modifier::BOLD)),
            Span::styled("·", Style::default().fg(Color::DarkGray)),
            Span::styled(" pending ", dim),
            Span::styled("✓", Style::default().fg(Color::Green)),
            Span::styled(" applied ", dim),
        ]),
    ]
}

// ── widget ────────────────────────────────────────────────────────────────────

/// A content line with metadata for the renderer.
enum ContentLine<'a> {
    /// A regular text line.
    Text(Line<'a>),
    /// A header row that should be rendered with Layout-based column centering.
    Header,
}

impl<'a> Widget for BatchesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let snap = self.snapshot;
        let has_prev = !snap.prev_step_batches.is_empty();
        let has_curr = !snap.step_batches.is_empty();

        if !has_prev && !has_curr {
            Paragraph::new(Span::styled(
                "No batch data for this step",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))
            .centered()
            .render(area, buf);
            return;
        }

        let w = area.width;
        let all_nodes = all_node_ids(snap);
        let (nodes_w, applied_w) = column_layout(w, all_nodes.len());

        let prev_step = snap
            .coordinator
            .as_ref()
            .map(|c| c.step.saturating_sub(1))
            .unwrap_or(0);
        let curr_step = snap.coordinator.as_ref().map(|c| c.step).unwrap_or(0);

        // Collect all content lines. Track the first line index belonging to
        // the selected node's assigned batch for auto-scroll targeting.
        let mut all_lines: Vec<ContentLine> = Vec::new();
        let mut first_selected_line: Option<usize> = None;

        let ctx = SectionCtx {
            all_nodes: &all_nodes,
            selected_node_id: self.selected_node_id,
            applied_by: &snap.prev_applied_by,
        };

        if has_prev {
            all_lines.push(ContentLine::Text(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("step {} (previous)", prev_step),
                    Style::default().fg(Color::Cyan).bold(),
                ),
            ])));
            all_lines.push(ContentLine::Header);
            for tagged in
                build_section_data_lines(&snap.prev_step_batches, &ctx, nodes_w, applied_w)
            {
                if tagged.is_selected_batch && first_selected_line.is_none() {
                    first_selected_line = Some(all_lines.len());
                }
                all_lines.push(ContentLine::Text(tagged.line));
            }
            all_lines.push(ContentLine::Text(Line::from("")));
        }
        if has_curr {
            let ctx_curr = SectionCtx {
                all_nodes: &all_nodes,
                selected_node_id: self.selected_node_id,
                applied_by: &snap.applied_by,
            };
            all_lines.push(ContentLine::Text(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("step {} (current)", curr_step),
                    Style::default().fg(Color::Cyan).bold(),
                ),
            ])));
            all_lines.push(ContentLine::Header);
            for tagged in
                build_section_data_lines(&snap.step_batches, &ctx_curr, nodes_w, applied_w)
            {
                if tagged.is_selected_batch && first_selected_line.is_none() {
                    first_selected_line = Some(all_lines.len());
                }
                all_lines.push(ContentLine::Text(tagged.line));
            }
        }

        for l in build_witness_lines(snap, self.selected_node_id) {
            all_lines.push(ContentLine::Text(l));
        }
        for l in build_legend_lines() {
            all_lines.push(ContentLine::Text(l));
        }

        // Auto-scroll: centre the selected node's first batch row in the viewport.
        let visible_h = area.height as usize;
        let total = all_lines.len();
        let scroll = if let Some(target) = first_selected_line {
            // Try to put the target line ~1/3 from the top for context.
            let ideal_top = target.saturating_sub(visible_h / 3);
            let max_scroll = total.saturating_sub(visible_h);
            ideal_top.min(max_scroll)
        } else {
            0
        };

        // Render visible lines.
        for i in 0..visible_h {
            let line_idx = scroll + i;
            if line_idx >= total {
                break;
            }
            let line_y = area.y + i as u16;
            let line_rect = Rect {
                x: area.x,
                y: line_y,
                width: area.width,
                height: 1,
            };

            match &all_lines[line_idx] {
                ContentLine::Header => render_section_header(line_rect, buf, nodes_w, applied_w),
                ContentLine::Text(line) => {
                    Paragraph::new(line.clone()).render(line_rect, buf);
                }
            }
        }
    }
}
