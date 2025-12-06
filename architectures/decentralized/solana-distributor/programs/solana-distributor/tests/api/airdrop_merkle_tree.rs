use anyhow::Result;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::MerkleHash;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct AirdropMerkleTree {
    allocations: Vec<Allocation>,
    merkle_layers: Vec<Vec<MerkleHash>>,
}

impl AirdropMerkleTree {
    pub fn try_from(
        allocations: &Vec<Allocation>,
    ) -> Result<AirdropMerkleTree> {
        if allocations.is_empty() {
            return Err(anyhow::anyhow!("Allocations must not be empty"));
        }
        let mut merkle_layers = vec![];
        let mut merkle_layer = vec![];
        for allocation in allocations {
            merkle_layer.push(allocation.to_merkle_hash());
        }
        merkle_layers.push(merkle_layer.clone());
        while merkle_layer.len() > 1 {
            let mut merkle_parents = vec![];
            for pair in merkle_layer.chunks(2) {
                let left = &pair[0];
                let right = if pair.len() == 2 { &pair[1] } else { &pair[0] };
                merkle_parents.push(MerkleHash::from_pair(left, right));
            }
            merkle_layers.push(merkle_parents.clone());
            merkle_layer = merkle_parents;
        }
        Ok(AirdropMerkleTree {
            allocations: allocations.to_vec(),
            merkle_layers,
        })
    }

    pub fn root(&self) -> Result<&MerkleHash> {
        self.merkle_layers
            .last()
            .and_then(|layer| layer.first())
            .ok_or_else(|| anyhow::anyhow!("Merkle tree is empty"))
    }

    pub fn allocations(&self) -> &[Allocation] {
        &self.allocations
    }

    pub fn allocations_indexes_for_claimer(
        &self,
        claimer: &Pubkey,
    ) -> Result<Vec<usize>> {
        let mut indexes = vec![];
        for (position, allocation) in self.allocations.iter().enumerate() {
            if &allocation.claimer == claimer {
                indexes.push(position);
            }
        }
        Ok(indexes)
    }

    pub fn proof_at_allocation_index(
        &self,
        mut index: usize,
    ) -> Result<Vec<MerkleHash>> {
        let mut proof = vec![];
        for layer in &self.merkle_layers {
            if layer.len() == 1 {
                break;
            }
            let sibling_index =
                if index % 2 == 0 { index + 1 } else { index - 1 };
            if sibling_index >= layer.len() {
                proof.push(layer[index].clone());
            } else {
                proof.push(layer[sibling_index].clone());
            }
            index /= 2;
        }
        Ok(proof)
    }
}
