use std::fmt::{Debug, Display};

use anchor_lang::{Space, prelude::*};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Clone,
    Copy,
    Default,
    Zeroable,
    Pod,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
    PartialEq,
    Eq,
    Hash,
)]
#[repr(C)]
pub struct NodeIdentity {
    pub signer: [u8; 32],
    pub p2p_identity: [u8; 32],
}

impl NodeIdentity {
    pub fn new(signer: [u8; 32], p2p_identity: [u8; 32]) -> Self {
        Self {
            signer,
            p2p_identity,
        }
    }

    /// In non-Solana usage, we don't have a signer - so
    /// both signer and p2p_identity are the same pubkey.
    pub fn from_single_key(key: [u8; 32]) -> Self {
        Self {
            signer: key,
            p2p_identity: key,
        }
    }

    pub fn get_p2p_public_key(&self) -> &[u8; 32] {
        &self.p2p_identity
    }
}

impl Display for NodeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display first 4 bytes of signer as hex
        for b in &self.signer[..4] {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl Debug for NodeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeIdentity(")?;
        for b in &self.signer[..4] {
            write!(f, "{:02x}", b)?;
        }
        write!(f, "/")?;
        for b in &self.p2p_identity[..4] {
            write!(f, "{:02x}", b)?;
        }
        write!(f, ")")
    }
}

impl AsRef<[u8]> for NodeIdentity {
    fn as_ref(&self) -> &[u8] {
        &self.signer
    }
}

impl Space for NodeIdentity {
    const INIT_SPACE: usize = 64;
}
