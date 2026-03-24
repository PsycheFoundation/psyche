use anchor_lang::prelude::*;
use bytemuck::Zeroable;

use crate::small_boolean::SmallBoolean;

/// Salt constants for deterministic shuffling
pub mod salts {
    pub const COMMITTEE: &str = "committee";
    pub const WITNESS: &str = "witness";
    pub const COOLDOWN: &str = "cooldown";
}

#[derive(Clone, Copy, Default, PartialEq, Zeroable, AnchorDeserialize, AnchorSerialize)]
#[cfg_attr(
    feature = "client",
    derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[repr(C)]
pub enum Committee {
    #[default]
    TieBreaker,
    Verifier,
    Trainer,
}

#[cfg(feature = "client")]
impl std::fmt::Display for Committee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Committee::TieBreaker => write!(f, "Tie breaker"),
            Committee::Verifier => write!(f, "Verifier"),
            Committee::Trainer => write!(f, "Trainer"),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Zeroable, AnchorDeserialize, AnchorSerialize)]
#[cfg_attr(
    feature = "client",
    derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[repr(C)]
pub struct CommitteeProof {
    pub committee: Committee,
    pub position: u64,
    pub index: u64,
}

#[derive(
    Clone, Copy, Default, PartialEq, Zeroable, AnchorDeserialize, AnchorSerialize, InitSpace,
)]
#[cfg_attr(
    feature = "client",
    derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[repr(C)]
pub struct WitnessProof {
    /// Position in virtual shuffle, as determined by seed
    pub position: u64,
    /// Index into epoch_state.clients of sender
    pub index: u64,
    /// Assertion of witness membership or non-membership
    pub witness: SmallBoolean,
}
