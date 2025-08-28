pub mod client;
pub mod server;
pub mod test_utils;

// Model Parameters
pub const WARMUP_TIME: u64 = 1200;
pub const MAX_ROUND_TRAIN_TIME: u64 = 500;
pub const ROUND_WITNESS_TIME: u64 = 3;
pub const COOLDOWN_TIME: u64 = 5;
