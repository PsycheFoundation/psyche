use std::fmt::Debug;

use anyhow::Result;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub trait Networkable: Serialize + for<'a> Deserialize<'a> + Debug + Send + Sync + 'static {
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        postcard::from_bytes(bytes).map_err(Into::into)
    }
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("postcard::to_stdvec is infallible")
    }

    fn to_chunks(&self, chunk_size: usize) -> Vec<Vec<u8>> {
        let bytes = self.to_bytes();
        bytes
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .map(|chunk| chunk.collect::<Vec<_>>())
            .collect::<Vec<_>>()
    }
}

impl<T: Serialize + for<'a> Deserialize<'a> + Debug + Send + Sync + 'static> Networkable for T {}
