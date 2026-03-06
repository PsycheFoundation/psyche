use indexmap::IndexMap;
use psyche_event_sourcing::projection::{ClusterSnapshot, WarmupPhase};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Widget},
};

use crate::app::NodeFileStats;
use crate::utils::{fmt_bytes, short_id};

pub struct NodeWidget<'a> {
    pub snapshot: &'a ClusterSnapshot,
    /// None = no node selected (show placeholder).
    pub selected_node_idx: Option<usize>,
    pub file_stats: &'a IndexMap<String, NodeFileStats>,
}

impl<'a> Widget for NodeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let node_entry = self
            .selected_node_idx
            .and_then(|i| self.snapshot.nodes.get_index(i));

        let Some((id, node)) = node_entry else {
            let msg = if self.selected_node_idx.is_none() {
                "Use ↑/↓ to select a node"
            } else {
                "No nodes"
            };
            Paragraph::new(msg).render(
                Rect {
                    y: area.y + area.height / 2,
                    height: 1,
                    ..area
                },
                buf,
            );
            return;
        };

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(vec![
            "Node ID: ".bold(),
            Span::from(short_id(id, 20)),
        ]));

        // Run state
        lines.push(Line::from(vec![
            Span::styled("Run State: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(
                node.run_state
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]));

        // Warmup phase
        let warmup = &node.warmup;
        let phase_str = format!("{}", warmup.phase);
        lines.push(Line::from(vec![
            Span::styled("Warmup: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(phase_str),
        ]));

        // Download progress bar (if downloading)
        if warmup.phase == WarmupPhase::Downloading {
            let (ratio, label) = if let Some(total) = warmup.download_total_bytes {
                if total > 0 {
                    let r = (warmup.download_bytes as f64 / total as f64).clamp(0.0, 1.0);
                    (r, format!("{}/{} bytes", warmup.download_bytes, total))
                } else {
                    (0.0, "0 bytes".to_string())
                }
            } else {
                (0.0, format!("{} bytes", warmup.download_bytes))
            };

            let gauge_row = area.y + lines.len() as u16;
            if gauge_row < area.y + area.height {
                let gauge_area = Rect {
                    x: area.x,
                    y: gauge_row,
                    width: area.width,
                    height: 1,
                };
                Gauge::default()
                    .gauge_style(Style::default().fg(Color::Cyan))
                    .ratio(ratio)
                    .label(label)
                    .render(gauge_area, buf);
                lines.push(Line::from("")); // placeholder to advance line count
            }
        }

        lines.push(Line::from(""));

        // Network throughput
        if node.network_tx_bps.is_some() || node.network_rx_bps.is_some() {
            lines.push(Line::from(vec![
                Span::styled("Network: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("↑ {}/s", fmt_bytes(node.network_tx_bps.unwrap_or(0))),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("↓ {}/s", fmt_bytes(node.network_rx_bps.unwrap_or(0))),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
            lines.push(Line::from(""));
        }

        // Events file size + lifetime write rate
        if let Some(stats) = self.file_stats.get(&node.node_id) {
            lines.push(Line::from(vec![
                Span::styled("Events: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(fmt_bytes(stats.total_bytes)),
                Span::raw("  "),
                Span::styled(
                    format!("{}/s", fmt_bytes(stats.bytes_per_sec)),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            lines.push(Line::from(""));
        }

        // Health check failures
        lines.push(Line::from(vec![
            Span::styled(
                "Health failures: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{}", node.health_check_steps.len())),
        ]));

        // Last error
        if let Some(msg) = &node.last_error {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Last error: ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(msg),
            ]));
        }

        Paragraph::new(lines).render(area, buf);
    }
}
