use std::fmt::Display;

use anchor_lang::{
    prelude::{borsh, thiserror},
    AnchorDeserialize, AnchorSerialize, InitSpace,
};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::serde_utils::{serde_deserialize_string, serde_serialize_string};

#[derive(thiserror::Error, Debug)]
#[error("string of length {} doesn't fit in FixedString<{}>", 0.0, 0.1)]
pub struct FixedStringError((usize, usize));

#[derive(
    Serialize,
    Deserialize,
    Clone,
    Copy,
    TS,
    AnchorSerialize,
    AnchorDeserialize,
    PartialEq,
    Eq,
    InitSpace,
    Zeroable,
)]
#[repr(C)]
pub struct FixedString<const L: usize>(
    #[serde(
        serialize_with = "serde_serialize_string",
        deserialize_with = "serde_deserialize_string"
    )]
    #[ts(as = "String")]
    [u8; L],
);

impl<const L: usize> std::fmt::Debug for FixedString<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let used_bytes = match self.0.iter().position(|&b| b == 0) {
            Some(null_pos) => null_pos,
            None => L,
        };

        let zero_bytes = L - used_bytes;

        let string_content = String::from(self);

        write!(
            f,
            "\"{}\" ({}/{} bytes, {} zeroes)",
            string_content, used_bytes, L, zero_bytes
        )
    }
}

impl<const L: usize> Display for FixedString<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self))
    }
}

impl<const L: usize> Default for FixedString<L> {
    fn default() -> Self {
        Self([0u8; L])
    }
}

impl<const L: usize> FixedString<L> {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn from_str_truncated(s: &str) -> Self {
        let mut array = [0u8; L];
        let bytes = s.as_bytes();
        let len = bytes.len().min(L);
        array[..len].copy_from_slice(&bytes[..len]);
        Self(array)
    }

    pub fn is_empty(&self) -> bool {
        self.0[0] == 0
    }
}

impl<const L: usize> TryFrom<&str> for FixedString<L> {
    type Error = FixedStringError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut array = [0u8; L];
        let bytes = s.as_bytes();
        if bytes.len() > L {
            return Err(FixedStringError((bytes.len(), L)));
        }
        array[..bytes.len()].copy_from_slice(bytes);
        Ok(Self(array))
    }
}

impl<const L: usize> From<&FixedString<L>> for String {
    fn from(value: &FixedString<L>) -> Self {
        let sliced = match value.0.iter().position(|&b| b == 0) {
            Some(null_pos) => &value.0[0..null_pos],
            None => &value.0,
        };
        String::from_utf8_lossy(sliced).to_string()
    }
}
