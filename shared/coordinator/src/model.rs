use std::fmt::Display;

use anchor_lang::prelude::*;
use bytemuck::Zeroable;

use crate::fixed_vec::FixedVec;

pub const CHECKPOINT_DATA_MAX_LEN: usize = 256;
/// Opaque byte blob holding serialized [`CheckpointData`].
pub type CheckpointBytes = FixedVec<u8, CHECKPOINT_DATA_MAX_LEN>;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Zeroable, Copy)]
#[cfg_attr(
    feature = "client",
    derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[repr(C)]
pub struct Model {
    pub max_seq_len: u32,
    pub cold_start_warmup_steps: u32,
    pub checkpoint_source: CheckpointSource,
    #[cfg_attr(feature = "client", serde(default))]
    pub checkpoint_data: FixedVec<u8, { CHECKPOINT_DATA_MAX_LEN }>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Zeroable, Copy, PartialEq, Default)]
#[cfg_attr(
    feature = "client",
    derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[repr(C)]
pub enum CheckpointSource {
    Ephemeral,
    #[default]
    Stored,
    P2P,
}

#[cfg(feature = "client")]
impl std::fmt::Display for CheckpointSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointSource::Ephemeral => write!(f, "Ephemeral"),
            CheckpointSource::Stored => write!(f, "Stored"),
            CheckpointSource::P2P => write!(f, "P2P"),
        }
    }
}

#[cfg(feature = "client")]
impl Model {
    pub fn dummy(checkpoint_data: FixedVec<u8, { CHECKPOINT_DATA_MAX_LEN }>) -> Self {
        Self {
            checkpoint_source: CheckpointSource::Stored,
            checkpoint_data,
            max_seq_len: 2048,
            cold_start_warmup_steps: 0,
        }
    }
}

#[derive(Debug)]
pub enum ModelError {
    ZeroSeqLen,
    CheckpointEphemeral,
    CheckpointEmpty,
}

impl Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ModelError::ZeroSeqLen => "model check failed: max_seq_len is 0.",
                ModelError::CheckpointEphemeral =>
                    "model check failed: ephemeral checkpoint not allowed",
                ModelError::CheckpointEmpty => "model check failed: checkpoint data is empty",
            }
        )
    }
}

impl Model {
    pub fn check(&self) -> bool {
        self.check_error().is_ok()
    }

    #[inline(always)]
    pub fn check_error(&self) -> std::result::Result<(), ModelError> {
        if self.max_seq_len == 0 {
            return Err(ModelError::ZeroSeqLen);
        }

        if matches!(self.checkpoint_source, CheckpointSource::Ephemeral) {
            return Err(ModelError::CheckpointEphemeral);
        }

        if self.checkpoint_data.is_empty() {
            return Err(ModelError::CheckpointEmpty);
        }

        Ok(())
    }
}
