use std::fmt::Debug;

use anchor_lang::prelude::*;
use bytemuck::Pod;
use bytemuck::Zeroable;
use psyche_coordinator::node_identity::NodeIdentity;

#[derive(
    Clone,
    Copy,
    Default,
    Zeroable,
    InitSpace,
    Pod,
    AnchorSerialize,
    AnchorDeserialize,
)]
#[cfg_attr(
    feature = "client",
    derive(serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
#[cfg_attr(feature = "client", ts(rename = "SolanaClient"))]
#[repr(C)]
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
