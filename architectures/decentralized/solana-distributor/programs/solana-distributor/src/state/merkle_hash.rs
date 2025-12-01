use std::fmt;

use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub struct MerkleHash {
    bytes: [u8; 32],
}

impl MerkleHash {
    pub fn from_parts(vals: &[&[u8]]) -> MerkleHash {
        MerkleHash {
            bytes: hashv(vals).to_bytes(),
        }
    }

    pub fn from_pair(a: &MerkleHash, b: &MerkleHash) -> MerkleHash {
        MerkleHash {
            bytes: if a.bytes <= b.bytes {
                hashv(&[&a.bytes, &b.bytes]).to_bytes()
            } else {
                hashv(&[&b.bytes, &a.bytes]).to_bytes()
            },
        }
    }

    pub fn is_valid_proof(
        &self,
        merkle_proof: &[MerkleHash],
        merkle_root: &MerkleHash,
    ) -> bool {
        let mut merkle_hash = *self;
        for merkle_node in merkle_proof {
            merkle_hash = MerkleHash::from_pair(&merkle_hash, merkle_node);
        }
        merkle_hash == *merkle_root
    }
}

impl std::fmt::Debug for MerkleHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "0x{}",
            self.bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        )
    }
}
