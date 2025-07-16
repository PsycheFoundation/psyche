use std::collections::HashMap;

use psyche_coordinator::Committee;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    symbols,
    text::Line,
    widgets::{Axis, Chart, Dataset, GraphType, LegendPosition, Paragraph, Widget},
};
use psyche_watcher::TuiRunState;

lazy_static::lazy_static! {
    static ref GRAPH_COLORS: [Style; 4] = [Style::default().red(), Style::default().magenta(), Style::default().green(), Style::default().cyan()];
}

#[derive(Default, Debug)]
pub struct ClientTUI;

fn convert_tokens_per_sec(tokens_per_sec: f32) -> String {
    const KB: f32 = 1000.0;
    const MB: f32 = KB * 1000.0;
    const GB: f32 = MB * 1000.0;

    if tokens_per_sec == 0. {
        String::new()
    } else if tokens_per_sec < KB {
        format!("{tokens_per_sec} tok/s")
    } else if tokens_per_sec < MB {
        format!("{:.1}K tok/s", tokens_per_sec / KB)
    } else if tokens_per_sec < GB {
        format!("{:.1}M tok/s", tokens_per_sec / MB)
    } else {
        format!("{:.1}B tok/s", tokens_per_sec / GB)
    }
}

fn convert_tokens(tokens: u64) -> String {
    let tokens = tokens as f32;
    const KB: f32 = 1000.0;
    const MB: f32 = KB * 1000.0;
    const GB: f32 = MB * 1000.0;
    const TB: f32 = GB * 1000.0;

    if tokens < KB {
        format!("{tokens}")
    } else if tokens < MB {
        format!("{:.1}K", tokens / KB)
    } else if tokens < GB {
        format!("{:.1}M", tokens / MB)
    } else if tokens < TB {
        format!("{:.1}B", tokens / GB)
    } else {
        format!("{:.1}T", tokens / TB)
    }
}

impl psyche_tui::CustomWidget for ClientTUI {
    type Data = ClientTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let right_size = state
            .evals
            .keys()
            .map(|key| key.len())
            .max_by(|x, y| x.cmp(y))
            .unwrap_or(6) as u16;
        let coord_split = Layout::vertical(match state.evals.is_empty() {
            true => vec![Constraint::Fill(1), Constraint::Length(2)],
            false => vec![
                Constraint::Fill(1),
                Constraint::Length(2),
                Constraint::Fill(1),
            ],
        })
        .split(area);
        {
            let plot_split =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_size)])
                    .split(coord_split[0]);

            let x_max = state.step as f64;
            let x_min = x_max - (state.loss.len() as f64);
            let data = state
                .loss
                .iter()
                .enumerate()
                .map(|(i, val)| (i as f64 + x_min, *val as f64))
                .collect::<Vec<_>>();
            let y_min = 0f64.max(
                data.iter()
                    .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(&(0., 0.))
                    .1
                    - 0.1f64,
            );
            let y_max = y_min + 2.0;
            let dataset = Dataset::default()
                .name("Loss")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().cyan())
                .data(&data);
            let x_axis = Axis::default()
                .bounds([x_min, x_max])
                .labels([0.to_string(), x_max.to_string()])
                .style(Style::default().white());
            let y_axis = Axis::default()
                .bounds([y_min, y_max])
                .labels([format!("{y_min:.1}"), format!("{y_max:.1}")])
                .style(Style::default().white());
            Chart::new(vec![dataset])
                .x_axis(x_axis)
                .y_axis(y_axis)
                .legend_position(Some(LegendPosition::TopRight))
                .render(plot_split[0], buf);

            let vsplit = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .split(plot_split[1]);
            Paragraph::new(vec![
                Line::from("Loss"),
                Line::from(format!("{:.3}", state.loss.last().unwrap_or(&0.0))),
            ])
            .centered()
            .render(vsplit[1], buf);
        }
        {
            let plot_split =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_size)])
                    .split(coord_split[1]);

            let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
                .split(plot_split[0]);

            let top_row_layout =
                Layout::horizontal(Constraint::from_fills([1, 1, 1])).split(rows[0]);

            let bottom_row_layout =
                Layout::horizontal(Constraint::from_fills([1, 1])).split(rows[1]);

            Paragraph::new(format!(
                "State: {}",
                match state.run_state {
                    TuiRunState::RoundTrain =>
                        format!("{} ({})", state.run_state, state.batches_left),
                    _ => format!("{}", state.run_state),
                }
            ))
            .centered()
            .render(top_row_layout[0], buf);

            Paragraph::new(format!(
                "Efficency: {}%",
                (state.efficency * 100.0) as usize
            ))
            .centered()
            .render(top_row_layout[1], buf);

            Paragraph::new(format!(
                "Global Speed: {}",
                convert_tokens_per_sec(state.global_tokens_per_second)
            ))
            .centered()
            .render(top_row_layout[2], buf);

            Paragraph::new(format!(
                "Batch Size: {}",
                convert_tokens(state.token_batch_size as u64)
            ))
            .centered()
            .render(bottom_row_layout[0], buf);

            Paragraph::new(format!(
                "Total Tokens: {}",
                convert_tokens(state.total_tokens)
            ))
            .centered()
            .render(bottom_row_layout[1], buf);
        }
        if !state.evals.is_empty() {
            let plot_split =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_size)])
                    .split(coord_split[2]);

            let x_max = state.step as f64;
            let x_min = x_max - (state.evals.values().map(|x| x.len()).max().unwrap()) as f64;
            let y_min = 0.;
            let y_max = 1.;
            let plot_data: Vec<_> = state
                .evals
                .values()
                .map(|values| {
                    values
                        .iter()
                        .enumerate()
                        .map(|(i, val)| (i as f64 + x_min, *val))
                        .collect::<Vec<_>>()
                })
                .collect();
            let datasets: Vec<_> = state
                .evals
                .iter()
                .zip(plot_data.iter())
                .enumerate()
                .map(|(index, ((name, _), data))| {
                    Dataset::default()
                        .name(name.to_owned())
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(GRAPH_COLORS[index % GRAPH_COLORS.len()])
                        .data(data)
                })
                .collect();
            let x_axis = Axis::default()
                .bounds([x_min, x_max])
                .labels([0.to_string(), x_max.to_string()])
                .style(Style::default().white());
            let y_axis = Axis::default()
                .bounds([y_min, y_max])
                .labels([format!("{y_min:.1}"), format!("{y_max:.1}")])
                .style(Style::default().white());
            Chart::new(datasets)
                .x_axis(x_axis)
                .y_axis(y_axis)
                .legend_position(Some(LegendPosition::BottomRight))
                .render(plot_split[0], buf);

            let mut constraints = Vec::new();
            constraints.resize(state.evals.len(), Constraint::Length(4));
            constraints.insert(0, Constraint::Fill(1));
            constraints.push(Constraint::Fill(1));
            let vsplit = Layout::vertical(constraints).split(plot_split[1]);
            for (index, (name, value)) in state.evals.iter().enumerate() {
                Paragraph::new(vec![
                    Line::from(name.to_string()),
                    Line::from(format!("{:.3}", value.last().unwrap_or(&0.0))),
                ])
                .centered()
                .render(vsplit[index + 1], buf);
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ClientTUIState {
    pub step: u32,
    pub committee: Option<Committee>,
    pub run_state: TuiRunState,
    pub batches_left: usize,
    pub efficency: f32,
    pub loss: Vec<f32>,
    pub evals: HashMap<String, Vec<f64>>,
    pub global_tokens_per_second: f32,
    pub token_batch_size: u32,
    pub total_tokens: u64,
}
