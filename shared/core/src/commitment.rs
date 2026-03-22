use bytemuck::Zeroable;
use psyche_coordinator::coordinator::Witness;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Zeroable, Copy)]
#[repr(C)]
pub struct Commitment {
    pub data_hash: [u8; 32],
    pub signature: [u8; 64],
}

impl Serialize for Commitment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bytes = Vec::with_capacity(32 + 64);
        bytes.extend_from_slice(&self.data_hash);
        bytes.extend_from_slice(&self.signature);

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Commitment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = <Vec<_> as serde::Deserialize>::deserialize(deserializer)?;

        if bytes.len() != 96 {
            return Err(serde::de::Error::custom("Invalid length for Commitment"));
        }

        let mut data_hash = [0u8; 32];
        let mut signature = [0u8; 64];

        data_hash.copy_from_slice(&bytes[0..32]);
        signature.copy_from_slice(&bytes[32..96]);

        Ok(Commitment {
            data_hash,
            signature,
        })
    }
}

pub fn select_consensus_commitment_by_witnesses(
    commitments: &[Commitment],
    witnesses: &[Witness],
    witness_quorum: u16,
) -> Option<usize> {
    let mut scores = vec![0; commitments.len()];
    for witness in witnesses {
        for (index, commitment) in commitments.iter().enumerate() {
            if witness.broadcast_bloom.contains(&commitment.data_hash) {
                scores[index] += 1;
                break;
            }
        }
    }
    scores
        .into_iter()
        .enumerate()
        .filter(|(_, score)| *score >= witness_quorum)
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index)
}
