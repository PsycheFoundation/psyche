use std::{
    fmt::{Display, Formatter},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use psyche_coordinator::{Coordinator, RunState, model::Model};
use psyche_core::NodeIdentity;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct CoordinatorTui;

impl psyche_tui::CustomWidget for CoordinatorTui {
    type Data = CoordinatorTuiState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let coord_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);
        {
            let vsplit = Layout::vertical(Constraint::from_fills([1, 1])).split(coord_split[0]);
            {
                Paragraph::new(format!("{}", state.run_state))
                    .block(Block::bordered().title("Run state"))
                    .render(vsplit[0], buf);
            }
            {
                Paragraph::new(
                    state
                        .clients
                        .iter()
                        .map(|c| c.to_string().into())
                        .collect::<Vec<Line>>(),
                )
                .block(Block::bordered().title("Clients this round"))
                .render(vsplit[1], buf);
            }
        }
        {
            let vsplit = Layout::vertical(Constraint::from_fills([1, 1])).split(coord_split[1]);
            {
                Paragraph::new(
                    [format!("Model Checkpoint: {}", state.model_checkpoint)]
                        .into_iter()
                        .map(Line::from)
                        .collect::<Vec<_>>(),
                )
                .block(Block::bordered().title("Config"))
                .render(vsplit[0], buf);
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
                .render(vsplit[1], buf);
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub enum TuiRunState {
    #[default]
    Uninitialized,
    Paused,
    WaitingForMembers {
        need: u16,
    },
    Warmup {
        end_time: Instant,
    },
    RoundTrain,
    RoundWitness,
    Cooldown {
        end_time: Option<Instant>,
    },
    Finished,
}

impl Display for TuiRunState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TuiRunState::Uninitialized => write!(f, "Uninitialized"),
            TuiRunState::Paused => write!(f, "Paused"),
            TuiRunState::WaitingForMembers { need } => write!(f, "Waiting for {need} members"),
            TuiRunState::Warmup { end_time } => {
                let remaining = end_time.duration_since(Instant::now());
                write!(f, "Warmup ({}s remaining)", remaining.as_secs())
            }
            TuiRunState::RoundTrain => write!(f, "Training"),
            TuiRunState::RoundWitness => write!(f, "Witnessing"),
            TuiRunState::Cooldown { end_time } => match end_time {
                Some(end_time) => {
                    let remaining = end_time.duration_since(Instant::now());
                    write!(f, "Cooldown ({}s remaining)", remaining.as_secs())
                }
                None => write!(f, "Cooldown"),
            },
            TuiRunState::Finished => write!(f, "Finished"),
        }
    }
}

impl<T: NodeIdentity> From<&Coordinator<T>> for TuiRunState {
    fn from(c: &Coordinator<T>) -> Self {
        match c.run_state {
            RunState::Uninitialized => TuiRunState::Uninitialized,
            RunState::Paused => TuiRunState::Paused,
            RunState::WaitingForMembers => TuiRunState::WaitingForMembers {
                need: c
                    .config
                    .min_clients
                    .saturating_sub(c.epoch_state.clients.len() as u16),
            },
            RunState::Warmup => {
                let time_since_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO);

                TuiRunState::Warmup {
                    end_time: Instant::now()
                        + Duration::from_secs(
                            c.config.warmup_time + c.run_state_start_unix_timestamp,
                        )
                        - time_since_epoch,
                }
            }
            RunState::RoundTrain => TuiRunState::RoundTrain,
            RunState::RoundWitness => TuiRunState::RoundWitness,
            RunState::Cooldown => TuiRunState::Cooldown {
                end_time: match c.config.cooldown_time {
                    0 => None,
                    cooldown_time => {
                        let time_since_epoch = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or(Duration::ZERO);

                        Some(
                            Instant::now()
                                + Duration::from_secs(
                                    cooldown_time + c.run_state_start_unix_timestamp,
                                )
                                - time_since_epoch,
                        )
                    }
                },
            },
            RunState::Finished => TuiRunState::Finished,
        }
    }
}

#[derive(Default, Debug)]
pub struct CoordinatorTuiState {
    pub run_id: String,
    pub run_state: TuiRunState,
    pub height: u32,
    pub clients: Vec<String>,
    pub model_checkpoint: String,
    pub exited_clients: usize,
    pub pending_pause: bool,
}

impl<T: NodeIdentity> From<&Coordinator<T>> for CoordinatorTuiState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_id: (&value.run_id).into(),
            run_state: value.into(),
            height: value.epoch_state.rounds[value.epoch_state.rounds_head as usize].height,
            clients: value
                .epoch_state
                .clients
                .iter()
                .map(|c| format!("{:?}", c.id))
                .collect(),
            model_checkpoint: match &value.model {
                Model::LLM(l) => format!("{}", l.checkpoint),
            },
            exited_clients: value.epoch_state.exited_clients.len(),
            pending_pause: value.pending_pause.is_true(),
        }
    }
}
