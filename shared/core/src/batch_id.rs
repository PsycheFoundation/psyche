use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BatchId(u64);

impl fmt::Display for BatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", self.0)
    }
}

impl From<BatchId> for u64 {
    fn from(batch_id: BatchId) -> Self {
        batch_id.0
    }
}

impl BatchId {
    pub fn from_u64(batch_number: u64) -> Self {
        Self(batch_number)
    }
}