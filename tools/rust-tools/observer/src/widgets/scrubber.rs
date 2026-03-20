use psyche_event_sourcing::timeline::ClusterTimeline;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

pub struct ScrubberWidget<'a> {
    pub timeline: &'a ClusterTimeline,
    pub cursor: usize,
}

impl<'a> Widget for ScrubberWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total = self.timeline.len();
        let entries = self.timeline.entries();

        let (ts_start, ts_end, ts_cursor) =
            if let Some((start, end)) = self.timeline.timestamp_range() {
                let cursor_ts = entries
                    .get(self.cursor)
                    .map(|e| e.timestamp())
                    .unwrap_or(start);
                (
                    start.format("%H:%M:%S").to_string(),
                    end.format("%H:%M:%S").to_string(),
                    cursor_ts.format("%H:%M:%S%.3f").to_string(),
                )
            } else {
                ("--:--:--".into(), "--:--:--".into(), "--:--:--.---".into())
            };

        // ── Scrubber bar ───────────────────────────────────────────────────────
        let bar_width = area.width.saturating_sub(4) as usize;
        let pos = if total > 1 && bar_width > 0 {
            (self.cursor * bar_width / (total - 1)).min(bar_width)
        } else {
            0
        };

        let bar: String = std::iter::once('◄')
            .chain(std::iter::repeat_n('─', pos))
            .chain(std::iter::once('●'))
            .chain(std::iter::repeat_n('─', bar_width.saturating_sub(pos)))
            .chain(std::iter::once('►'))
            .collect();

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(Span::styled(
            bar,
            Style::default().fg(Color::Cyan),
        )));

        // ── Timestamp row ──────────────────────────────────────────────────────
        let count_str = format!("{}/{}", self.cursor + 1, total);
        let ts_mid = format!("{}  {}", ts_cursor, count_str);
        let side_width = (ts_start.len() + ts_end.len()) as u16;
        let mid_width = area.width.saturating_sub(side_width) as usize;
        lines.push(Line::from(vec![
            Span::styled(&ts_start, Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:^width$}", ts_mid, width = mid_width)),
            Span::styled(&ts_end, Style::default().fg(Color::DarkGray)),
        ]));

        // ── Keybinds ───────────────────────────────────────────────────────────
        lines.push(Line::from(Span::styled(
            "[←/→] step  [Shift+←/→] ×50  [Space] play  [↑/↓] node  [1/2/3] speed  [g/G] first/last  [[/]] zoom  [q] quit",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));

        Paragraph::new(lines).render(area, buf);
    }
}
