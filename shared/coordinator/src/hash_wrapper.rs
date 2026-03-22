use anchor_lang::prelude::*;
use bytemuck::Zeroable;

/// This wrapper is used to implement the `Space` trait for the actual hash.
#[derive(
    AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Clone, Default, Zeroable, Copy, InitSpace,
)]
#[cfg_attr(
    feature = "client",
    derive(serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
pub struct HashWrapper {
    pub inner: [u8; 32],
}

impl HashWrapper {
    pub fn new(inner: [u8; 32]) -> Self {
        Self { inner }
    }

    #[cfg(feature = "client")]
    pub fn fmt_short(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.inner[..5])
    }

    #[cfg(feature = "client")]
    pub fn fmt_full(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.inner)
    }
}

#[cfg(feature = "client")]
impl std::fmt::Debug for HashWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HashWrapper({})", self.fmt_full())
    }
}

impl AsRef<[u8]> for HashWrapper {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}
