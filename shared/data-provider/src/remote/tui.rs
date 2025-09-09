use psyche_core::NodeIdentity;
use psyche_network::AuthenticatableIdentity;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Block, Gauge, Paragraph, Widget},
};
use psyche_watcher::Backend;

use crate::{TokenizedDataProvider, traits::LengthKnownDataProvider};

use super::DataProviderTcpServer;

#[derive(Default, Debug)]
pub struct DataServerTui;

impl psyche_tui::CustomWidget for DataServerTui {
    type Data = DataServerTuiState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let global_stats =
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);

        {
            {
                let split =
                    Layout::horizontal(Constraint::from_fills([1, 1])).split(global_stats[0]);
                Paragraph::new(Text::from(vec![
                    Line::from(format!("Total samples: {}", state.total_samples)),
                    Line::from(format!("Provided samples: {}", state.given_samples)),
                ]))
                .block(Block::bordered().title("Stats"))
                .render(split[0], buf);

                Gauge::default()
                    .block(Block::bordered().title("Percent of data given out"))
                    .ratio(state.given_samples as f64 / state.total_samples as f64)
                    .render(split[1], buf);
            }
        }

        {
            let coord_split =
                Layout::horizontal(Constraint::from_fills([1, 1])).split(global_stats[1]);
            {
                Paragraph::new(
                    state
                        .clients
                        .iter()
                        .map(|c| {
                            // let status = if c.2 { "⏳" } else { "✅" };
                            // Line::from(format!("{status} [{}]: {:?}", c.0, c.1))
                            Line::from(format!("[{}]: {}", c.0, c.1))
                        })
                        .collect::<Vec<Line>>(),
                )
                .block(Block::bordered().title("Clients"))
                .render(coord_split[0], buf);
            }
            {
                Paragraph::new(
                    [
                        format!("Clients: {:?}", state.clients.len()),
                        format!("Height: {:?}", state.height),
                    ]
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
                )
                .block(Block::bordered().title("Current state"))
                .render(coord_split[1], buf);
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct DataServerTuiState {
    pub height: u32,
    //pub clients: Vec<(String, [u64; 2], bool)>,
    pub clients: Vec<(String, usize)>,

    pub total_samples: usize,
    pub given_samples: usize,
}

impl<T, A, D, W> From<&DataProviderTcpServer<T, A, D, W>> for DataServerTuiState
where
    T: NodeIdentity,
    A: AuthenticatableIdentity,
    D: TokenizedDataProvider + LengthKnownDataProvider,
    W: Backend<T>,
{
    fn from(v: &DataProviderTcpServer<T, A, D, W>) -> Self {
        Self {
            height: v
                .state
                .current_round()
                .map(|x| x.height)
                .unwrap_or_default(),
            // clients: v
            //     .selected_data
            //     .iter()
            //     .map(|(data_ids, client_id)| {
            //         let id = format!("{}", client_id);
            //         let data_ids = [data_ids.start, data_ids.end];
            //         let has_fetched =
            //             (data_ids[0]..data_ids[1] + 1)
            //                 .all(|val| *v.provided_sequences.get(&(val as usize)).unwrap_or(&false));
            //         (id, data_ids, has_fetched)
            //     })
            //     .collect(),
            clients: v
                .provided_sequences
                .iter()
                .map(|(k, v)| (format!("{k}"), *v))
                .collect(),
            total_samples: v.local_data_provider.num_sequences(),
            given_samples: v.provided_sequences.values().fold(0, |acc, ele| acc + *ele),
        }
    }
}
