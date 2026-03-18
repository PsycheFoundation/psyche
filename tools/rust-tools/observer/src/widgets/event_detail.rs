use std::collections::HashSet;

use psyche_event_sourcing::timeline::{ClusterTimeline, TimelineEntry};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use super::waterfall::{EventCategory, entry_matches_filter};
use crate::utils::short_id;

pub struct EventDetailWidget<'a> {
    pub timeline: &'a ClusterTimeline,
    pub cursor: usize,
    pub selected_node_id: Option<&'a str>,
    pub filter: &'a HashSet<EventCategory>,
}

impl<'a> Widget for EventDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        let entries = self.timeline.entries();
        let entry = entries.get(self.cursor).filter(|e| {
            entry_matches_filter(e, self.filter)
                && match (e, self.selected_node_id) {
                    (_, None) => true,
                    (TimelineEntry::Node { node_id, .. }, Some(sel)) => node_id.as_str() == sel,
                    (TimelineEntry::Coordinator { .. }, Some(sel)) => sel == "coordinator",
                }
        });

        let Some(entry) = entry else {
            let msg = if self.selected_node_id.is_some() {
                "No matching event at cursor for selected node"
            } else {
                "No matching event at cursor"
            };
            Paragraph::new(Span::styled(
                msg,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))
            .centered()
            .render(area, buf);
            return;
        };

        let (title, debug_str, ts, entity_label) = match entry {
            TimelineEntry::Node { event, node_id, .. } => (
                format!("{}", event.data),
                format!("{:#?}", event.data),
                event.timestamp,
                short_id(node_id, 12),
            ),
            TimelineEntry::Coordinator { state, timestamp } => (
                format!("coordinator e{} s{}", state.epoch, state.step),
                format!("{:#?}", state),
                *timestamp,
                "coordinator".to_string(),
            ),
        };

        let mut lines: Vec<Line> = Vec::new();

        // Title line.
        lines.push(Line::from(vec![Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));

        // Entity + timestamp.
        lines.push(
            Line::from(format!(" {} · {}", entity_label, ts.format("%H:%M:%S%.3f")))
                .style(Style::default().fg(Color::DarkGray)),
        );

        lines.push(Line::from(""));

        // Debug dump.
        for l in debug_str.lines() {
            lines.push(Line::from(format!(" {l}")).style(Style::default().fg(Color::Gray)));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
