use psyche_event_sourcing::projection::ClusterSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

pub struct CoordinatorBarWidget<'a> {
    pub snapshot: &'a ClusterSnapshot,
}

impl<'a> Widget for CoordinatorBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(c) = self.snapshot.coordinator.as_ref() else {
            Paragraph::new(Span::styled(
                "no coordinator data",
                Style::default().fg(Color::DarkGray),
            ))
            .render(area, buf);
            return;
        };

        let kv = |k: &'static str, v: String, color: Color| -> Vec<Span<'static>> {
            vec![
                Span::styled(k, Style::default().fg(Color::DarkGray)),
                Span::styled(v, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
            ]
        };

        let mut line1_spans: Vec<Span> = Vec::new();
        line1_spans.extend(kv("epoch: ", c.epoch.to_string(), Color::White));
        line1_spans.extend(kv("step: ", c.step.to_string(), Color::White));
        line1_spans.extend(kv("state: ", format!("{:?}", c.run_state), Color::Green));
        line1_spans.extend(kv(
            "checkpoint: ",
            format!("{:?}", c.checkpoint),
            Color::Cyan,
        ));
        line1_spans.extend(kv(
            "clients: ",
            format!("{}/{}", c.client_ids.len(), c.min_clients),
            if c.client_ids.len() >= c.min_clients {
                Color::Green
            } else {
                Color::Yellow
            },
        ));

        if area.height >= 1 {
            Paragraph::new(Line::from(line1_spans)).render(Rect { height: 1, ..area }, buf);
        }
    }
}
