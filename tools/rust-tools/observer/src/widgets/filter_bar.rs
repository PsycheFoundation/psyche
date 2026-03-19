use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use strum::IntoEnumIterator;

use super::waterfall::EventCategory;

/// Separator row that doubles as a category filter picker.
///
/// Fills the row with `═` and overlays right-aligned filter toggles.
pub struct FilterBarWidget<'a> {
    pub filter: &'a HashSet<EventCategory>,
}

impl<'a> Widget for FilterBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill with ═ as background.
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, "═", Style::default().fg(Color::DarkGray));
        }

        let filterable: Vec<EventCategory> =
            EventCategory::iter().filter(|c| c.filterable()).collect();

        // Build picker spans.
        let picker_spans: Vec<Span> = filterable
            .iter()
            .flat_map(|&cat| {
                let active = self.filter.contains(&cat);
                let base = if active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(cat.color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(cat.color())
                };
                let key_style = base.add_modifier(Modifier::BOLD);

                let label = cat.label();
                let key = cat.key();
                let key_pos = label.find(key).unwrap_or(0);
                let before = &label[..key_pos];
                let after = &label[key_pos + key.len_utf8()..];

                let mut spans: Vec<Span> = vec![Span::styled("■", base), Span::styled(" ", base)];
                if !before.is_empty() {
                    spans.push(Span::styled(before.to_string(), base));
                }
                spans.push(Span::styled(format!("({key})"), key_style));
                if !after.is_empty() {
                    spans.push(Span::styled(after.to_string(), base));
                }
                spans.push(Span::styled(" ", base));
                spans
            })
            .collect();

        let picker_w: u16 = filterable
            .iter()
            .map(|c| c.label().len() + 5)
            .sum::<usize>() as u16;

        if picker_w <= area.width {
            let px = area.x + area.width.saturating_sub(picker_w);
            Paragraph::new(Line::from(picker_spans)).render(
                Rect {
                    x: px,
                    y: area.y,
                    width: picker_w,
                    height: 1,
                },
                buf,
            );
        }
    }
}
