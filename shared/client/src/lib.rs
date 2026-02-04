mod cli;
mod client;
mod fetch_data;
mod protocol;
mod state;
mod tui;

pub use cli::{TrainArgs, prepare_environment, print_identity_keys, read_identity_secret_key};
pub use client::Client;
pub use protocol::{Broadcast, BroadcastType, Finished, NC, TrainingResult};
pub use state::{
    CheckpointConfig, HubUploadInfo, InitRunError, RoundState, RunInitConfig, RunInitConfigAndIO,
};
pub use tui::{ClientTUI, ClientTUIState};

#[derive(Clone, Debug)]
pub struct WandBInfo {
    pub project: String,
    pub run: String,
    pub group: Option<String>,
    pub entity: Option<String>,
    pub api_key: String,
}
