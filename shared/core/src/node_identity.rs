use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use anchor_lang::{AnchorDeserialize, AnchorSerialize, Space};
use bytemuck::Zeroable;
use serde::{de::DeserializeOwned, Serialize};

pub trait NodeIdentity:
    Display
    + Copy
    + Debug
    + PartialEq
    + Eq
    + Hash
    + AsRef<[u8]>
    + Clone
    + Send
    + Sync
    + Space
    + Zeroable
    + Default
    + Serialize
    + AnchorDeserialize
    + AnchorSerialize
    + DeserializeOwned
    + 'static
{
    fn get_p2p_public_key(&self) -> &[u8; 32];
}
