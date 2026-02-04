use anchor_lang::{AnchorDeserialize, AnchorSerialize, InitSpace, prelude::borsh};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    PartialEq,
    TS,
)]
#[repr(C)]
#[derive(Default)]
pub enum Shuffle {
    #[default]
    DontShuffle,
    Seeded([u8; 32]),
}
