use std::fmt::Debug;

use anchor_lang::prelude::*;
use bytemuck::Pod;
use bytemuck::Zeroable;
use psyche_core::NodeIdentity;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(
    Clone,
    Copy,
    Default,
    Zeroable,
    InitSpace,
    Pod,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[repr(C)]
#[ts(rename = "SolanaClient")]
pub struct Client {
    pub id: NodeIdentity,
    pub _unused: [u8; 8],
    pub earned: u64,
    pub slashed: u64,
    pub active: u64,
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("earned", &self.earned)
            .field("slashed", &self.slashed)
            .field("active", &self.active)
            .finish()
    }
}
