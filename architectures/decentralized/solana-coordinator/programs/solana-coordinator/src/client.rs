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
    PartialEq,
    TS,
)]
#[repr(C)]
#[ts(rename = "SolanaClient")]
pub struct Client {
    pub id: NodeIdentity,
    #[ts(type = "number[]")]
    pub claimer: Pubkey,
    pub earned: u64,
    pub slashed: u64,
    pub active: u64,
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("claimer", &self.claimer)
            .field("earned", &self.earned)
            .field("slashed", &self.slashed)
            .field("active", &self.active)
            .finish()
    }
}
