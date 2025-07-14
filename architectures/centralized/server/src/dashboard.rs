use psyche_tui::{
    CustomWidget,
    crossterm::event::{Event, KeyCode, KeyModifiers},
    ratatui::{
        layout::{Constraint, Direction, Layout},
        text::Line,
        widgets::{Block, Paragraph, Widget},
    },
};
use psyche_watcher::{CoordinatorTuiState, TuiRunState};
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Default)]
pub struct DashboardState {
    pub server_addr: String,
    pub coordinator_state: CoordinatorTuiState,
    pub nodes_next_epoch: Vec<String>,
}

#[derive(Default)]
pub struct DashboardTui {
    pub pause: Arc<Notify>,
}

impl CustomWidget for DashboardTui {
    type Data = DashboardState;

    fn render(
        &mut self,
        area: psyche_tui::ratatui::prelude::Rect,
        buf: &mut psyche_tui::ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let vertical = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).split(area);

        {
            let title_split =
                Layout::horizontal(Constraint::from_fills([1, 1, 1])).split(vertical[0]);

            Paragraph::new(state.server_addr.clone())
                .block(Block::bordered().title("Server Address"))
                .render(title_split[0], buf);
            Paragraph::new(state.coordinator_state.run_id.clone())
                .block(Block::bordered().title("Run ID"))
                .render(title_split[1], buf);
            Paragraph::new(match state.coordinator_state.pending_pause {
                true => "Pending pause...",
                false => match state.coordinator_state.run_state {
                    TuiRunState::Paused => "Ctrl + P to resume",
                    _ => "Ctrl + P to pause",
                },
            })
            .centered()
            .block(Block::bordered())
            .render(title_split[2], buf);
        }
        {
            let coord_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(Constraint::from_fills([1, 1]))
                .split(vertical[1]);

            {
                let vsplit = Layout::vertical(Constraint::from_fills([1, 1])).split(coord_split[0]);
                Paragraph::new(format!("{}", state.coordinator_state.run_state))
                    .block(Block::bordered().title("Run state"))
                    .render(vsplit[0], buf);
                Paragraph::new(
                    state
                        .nodes_next_epoch
                        .iter()
                        .cloned()
                        .map(Line::from)
                        .collect::<Vec<_>>(),
                )
                .block(Block::bordered().title("Clients next round"))
                .render(vsplit[1], buf);
            }

            Paragraph::new(
                [
                    format!(
                        "Clients: {} ({} exited)",
                        state.coordinator_state.clients.len(),
                        state.coordinator_state.exited_clients
                    ),
                    format!("Height: {}", state.coordinator_state.height),
                    format!("Checkpoint: {}", state.coordinator_state.model_checkpoint),
                ]
                .into_iter()
                .map(Line::from)
                .collect::<Vec<_>>(),
            )
            .block(Block::bordered().title("Coordinator info"))
            .render(coord_split[1], buf);
        }
    }

    fn on_ui_event(&mut self, event: &Event) {
        if let Event::Key(key_event) = event {
            if key_event.code == KeyCode::Char('p') && key_event.modifiers == KeyModifiers::CONTROL
            {
                self.pause.notify_one();
            }
        }
    }
}
