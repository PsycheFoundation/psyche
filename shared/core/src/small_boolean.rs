use std::fmt::Debug;

use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Zeroable,
    Pod,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    TS,
)]
#[repr(transparent)]
pub struct SmallBoolean(pub u8);

impl Debug for SmallBoolean {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_true() {
            write!(f, "SmallBoolean(true)")
        } else {
            write!(f, "SmallBoolean(false)")
        }
    }
}

impl SmallBoolean {
    pub const TRUE: SmallBoolean = SmallBoolean(1);
    pub const FALSE: SmallBoolean = SmallBoolean(0);

    pub fn new(value: bool) -> Self {
        if value {
            Self::TRUE
        } else {
            Self::FALSE
        }
    }

    pub fn is_false(&self) -> bool {
        self.0 == 0
    }

    pub fn is_true(&self) -> bool {
        !self.is_false()
    }
}

impl From<bool> for SmallBoolean {
    fn from(b: bool) -> Self {
        Self::new(b)
    }
}

impl From<SmallBoolean> for bool {
    fn from(b: SmallBoolean) -> Self {
        b.is_true()
    }
}

impl std::ops::Not for SmallBoolean {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::new(!self.is_true())
    }
}

impl std::fmt::Display for SmallBoolean {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", if self.is_true() { "true" } else { "false" })
    }
}

impl Default for SmallBoolean {
    fn default() -> Self {
        Self::FALSE
    }
}
