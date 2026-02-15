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
    pub id: ClientId,
    #[ts(type = "number[]")]
    pub claimer: Pubkey,
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

#[derive(
    InitSpace,
    Copy,
    Clone,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    Default,
    Zeroable,
    Pod,
    TS,
)]
#[repr(C)]
pub struct ClientId {
    #[ts(type = "number[]")]
    pub signer: Pubkey,
    pub p2p_identity: [u8; 32],
}

impl Debug for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientId")
            .field("signer", &self.signer)
            .field("p2p_identity", &Pubkey::new_from_array(self.p2p_identity))
            .finish()
    }
}

impl AsRef<[u8]> for ClientId {
    fn as_ref(&self) -> &[u8] {
        self.signer.as_ref()
    }
}

impl std::hash::Hash for ClientId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signer.hash(state);
    }
}

impl std::cmp::PartialEq for ClientId {
    fn eq(&self, other: &Self) -> bool {
        self.signer == other.signer
    }
}

impl std::cmp::Eq for ClientId {}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.signer)
    }
}

impl NodeIdentity for ClientId {
    fn get_p2p_public_key(&self) -> &[u8; 32] {
        &self.p2p_identity
    }
}

impl ClientId {
    pub fn new(signer: Pubkey, p2p_identity: [u8; 32]) -> Self {
        Self {
            signer,
            p2p_identity,
        }
    }
}
