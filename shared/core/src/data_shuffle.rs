use serde::{Deserialize, Serialize};

/// Controls how data should be shuffled
/// - DontShuffle: Keep original order
/// - Seeded: Use provided seed for deterministic shuffling
#[derive(Serialize, Deserialize, Clone, Debug, Copy)]
#[repr(C)]
pub enum Shuffle {
    DontShuffle,
    Seeded([u8; 32]),
}