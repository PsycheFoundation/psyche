use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Copy)]
#[repr(C)]
pub enum TokenSize {
    TwoBytes,
    FourBytes,
}

impl From<TokenSize> for usize {
    fn from(value: TokenSize) -> Self {
        match value {
            TokenSize::TwoBytes => 2,
            TokenSize::FourBytes => 4,
        }
    }
}

impl TryFrom<usize> for TokenSize {
    type Error = anyhow::Error;

    fn try_from(value: usize) -> std::result::Result<Self, Self::Error> {
        match value {
            2 => Ok(Self::TwoBytes),
            4 => Ok(Self::FourBytes),
            x => Err(anyhow!("Unsupported token bytes length {x}")),
        }
    }
}
