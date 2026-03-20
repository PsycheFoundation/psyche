use indexmap::IndexMap;
use psyche_event_sourcing::projection::NodeSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, LegendPosition, Paragraph, Widget},
};

use crate::utils::short_id;

static NODE_COLORS: &[Color] = &[
    Color::Cyan,
    Color::Red,
    Color::Magenta,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::LightCyan,
    Color::LightRed,
    Color::LightMagenta,
    Color::LightGreen,
];

pub struct LossGraphWidget<'a> {
    pub nodes: &'a IndexMap<String, NodeSnapshot>,
}

struct Node {
    label: String,
    color_idx: usize,

    // loss chart (x, y)
    points: Vec<(f64, f64)>,

    // (midpoint_step_f64, new_epoch_number)
    boundaries: Vec<(f64, u64)>,
}

impl<'a> Widget for LossGraphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" LOSS ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        let node_data: Vec<Node> = self
            .nodes
            .iter()
            .filter(|(_, n)| !n.losses.is_empty())
            .enumerate()
            .map(|(color_idx, (id, n))| {
                let short_id = short_id(id, 14);

                let pts: Vec<(f64, f64)> = n
                    .losses
                    .iter()
                    .map(|&(step, loss)| (step as f64, loss))
                    .collect();

                // Epoch info is no longer stored per loss entry; no boundaries to draw.
                let boundaries: Vec<(f64, u64)> = Vec::new();

                Node {
                    boundaries,
                    color_idx,
                    label: short_id,
                    points: pts,
                }
            })
            .collect();

        if node_data.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "No loss data yet",
                Style::default().fg(Color::DarkGray),
            )))
            .centered()
            .render(inner, buf);
            return;
        }

        // Axis bounds across all nodes.
        let all_x: Vec<f64> = node_data
            .iter()
            .flat_map(|Node { points, .. }| points.iter().map(|(x, _)| *x))
            .collect();
        let all_y: Vec<f64> = node_data
            .iter()
            .flat_map(|Node { points, .. }| points.iter().map(|(_, y)| *y))
            .collect();

        let x_min = all_x.iter().copied().fold(f64::INFINITY, f64::min);
        let x_max = all_x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let y_lo = all_y.iter().copied().fold(f64::INFINITY, f64::min);
        let y_hi = all_y.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let y_margin = ((y_hi - y_lo) * 0.1).max(0.05);
        let y_min = (y_lo - y_margin).max(0.0);
        let y_max = if (y_hi + y_margin - y_min).abs() < f64::EPSILON {
            y_min + 1.0
        } else {
            y_hi + y_margin
        };
        let (x_min, x_max) = if (x_max - x_min).abs() < f64::EPSILON {
            (x_min - 0.5, x_max + 0.5)
        } else {
            (x_min, x_max)
        };

        // Build per-node loss line datasets.
        let mut datasets: Vec<Dataset> = node_data
            .iter()
            .map(
                |Node {
                     label,
                     color_idx,
                     points,
                     ..
                 }| {
                    Dataset::default()
                        .name(label.as_str())
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Style::default().fg(NODE_COLORS[color_idx % NODE_COLORS.len()]))
                        .data(points)
                },
            )
            .collect();

        // Split inner: epoch ruler (1 row, top) + chart area (rest).
        if inner.height < 3 {
            return;
        }
        let epoch_ruler = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        let chart_area = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: inner.height - 1,
        };

        // Column geometry — needed to filter boundaries by pixel distance.
        // The chart reserves `y_label_max_len + 1` columns for the y-axis on the left.
        let y_label_max_len = format!("{:.2}", y_max)
            .len()
            .max(format!("{:.2}", y_min).len()) as u16;
        let plot_x_start = chart_area.x + y_label_max_len + 1;
        let plot_width = chart_area.width.saturating_sub(y_label_max_len + 1);

        // Collect unique boundaries (x-value dedup), compute their column, then
        // drop any that land within MIN_BOUNDARY_COLS of the previous accepted one.
        const MIN_BOUNDARY_COLS: u16 = 10;
        let x_range = x_max - x_min;

        let mut seen_x: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        let mut raw_boundaries: Vec<(f64, u64, u16)> = node_data
            .iter()
            .flat_map(|Node { boundaries, .. }| boundaries.iter().copied())
            .filter_map(|(bx, epoch)| {
                let key = (bx * 10.0).round() as i64;
                if !seen_x.insert(key) || x_range <= f64::EPSILON || plot_width == 0 {
                    return None;
                }
                let col = plot_x_start + ((bx - x_min) / x_range * plot_width as f64) as u16;
                if col < epoch_ruler.x + epoch_ruler.width {
                    Some((bx, epoch, col))
                } else {
                    None
                }
            })
            .collect();

        raw_boundaries.sort_by_key(|&(_, _, col)| col);

        let mut boundaries_to_draw: Vec<(f64, u64, u16)> = Vec::new();
        for entry in raw_boundaries {
            let col = entry.2;
            if boundaries_to_draw
                .last()
                .is_none_or(|&(_, _, prev_col)| col.saturating_sub(prev_col) >= MIN_BOUNDARY_COLS)
            {
                boundaries_to_draw.push(entry);
            }
        }

        // Epoch boundary vertical lines in the chart body — DarkGray braille, no legend entry.
        let boundary_point_vecs: Vec<Vec<(f64, f64)>> = boundaries_to_draw
            .iter()
            .map(|&(bx, _, _)| {
                (0..=80)
                    .map(|i| (bx, y_min + (y_max - y_min) * (i as f64) / 80.0))
                    .collect()
            })
            .collect();

        for pts in &boundary_point_vecs {
            datasets.push(
                Dataset::default()
                    .name("")
                    .marker(symbols::Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(Color::DarkGray))
                    .data(pts),
            );
        }

        // Step-count x-axis (bottom), loss y-axis.
        let x_axis = Axis::default()
            .bounds([x_min, x_max])
            .labels([
                format!("step {}", x_min as i64),
                format!("step {}", x_max as i64),
            ])
            .style(Style::default().white());

        let y_axis = Axis::default()
            .bounds([y_min, y_max])
            .labels([format!("{y_min:.2}"), format!("{y_max:.2}")])
            .style(Style::default().white());

        Chart::new(datasets)
            .x_axis(x_axis)
            .y_axis(y_axis)
            .legend_position(Some(LegendPosition::TopRight))
            .render(chart_area, buf);

        // Draw epoch boundary ticks in the epoch ruler row above the chart.
        for &(_, epoch, col) in &boundaries_to_draw {
            buf.set_string(
                col,
                epoch_ruler.y,
                "│",
                Style::default().fg(Color::DarkGray),
            );
            let label = format!("e{epoch}");
            let label_end = col + 1 + label.len() as u16;
            let visible_end = label_end.min(epoch_ruler.x + epoch_ruler.width);
            if col + 1 < visible_end {
                let chars: String = label
                    .chars()
                    .take((visible_end - col - 1) as usize)
                    .collect();
                buf.set_string(
                    col + 1,
                    epoch_ruler.y,
                    &chars,
                    Style::default().fg(Color::DarkGray),
                );
            }
        }
    }
}
