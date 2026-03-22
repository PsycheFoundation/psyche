use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, Debug, Copy, PartialEq, TS, Default)]
pub enum Shuffle {
    #[default]
    DontShuffle,
    Seeded([u8; 32]),
}
