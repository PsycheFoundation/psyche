mod client;
mod server;
mod shared;
mod tui;
use std::path::PathBuf;

pub use client::DataProviderTcpClient;
use psyche_core::TokenSize;
pub use server::DataProviderTcpServer;
pub use tui::DataServerTui;

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug)]
pub struct DataServerConfig {
    pub dir: PathBuf,
    pub token_size: TokenSize,
    pub seq_len: usize,
    pub shuffle_seed: [u8; 32],
}
