pub mod client;
pub mod server;
pub mod test_utils;

// Model Parameters for tests
pub const WARMUP_TIME: u64 = 60;
pub const MAX_ROUND_TRAIN_TIME: u64 = 5;
pub const ROUND_WITNESS_TIME: u64 = 2;
pub const COOLDOWN_TIME: u64 = 3;

// Model Parameters for simulations
pub const SIM_WARMUP_TIME: u64 = 1200;
pub const SIM_MAX_ROUND_TRAIN_TIME: u64 = 500;
pub const SIM_ROUND_WITNESS_TIME: u64 = 3;
pub const SIM_COOLDOWN_TIME: u64 = 5;
