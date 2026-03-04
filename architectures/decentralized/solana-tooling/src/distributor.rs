use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropCreateAccounts;
use psyche_solana_distributor::accounts::AirdropUpdateAccounts;
use psyche_solana_distributor::accounts::AirdropWithdrawAccounts;
use psyche_solana_distributor::accounts::ClaimCreateAccounts;
use psyche_solana_distributor::accounts::ClaimRedeemAccounts;
use psyche_solana_distributor::instruction::AirdropCreate;
use psyche_solana_distributor::instruction::AirdropUpdate;
use psyche_solana_distributor::instruction::AirdropWithdraw;
use psyche_solana_distributor::instruction::ClaimCreate;
use psyche_solana_distributor::instruction::ClaimRedeem;
use psyche_solana_distributor::logic::AirdropCreateParams;
use psyche_solana_distributor::logic::AirdropUpdateParams;
use psyche_solana_distributor::logic::AirdropWithdrawParams;
use psyche_solana_distributor::logic::ClaimCreateParams;
use psyche_solana_distributor::logic::ClaimRedeemParams;
use psyche_solana_distributor::state::Airdrop;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::Claim;
use psyche_solana_distributor::state::MerkleHash;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub fn find_pda_airdrop(airdrop_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[Airdrop::SEEDS_PREFIX, &airdrop_id.to_le_bytes()],
        &psyche_solana_distributor::ID,
    )
    .0
}

pub fn find_pda_claim(
    airdrop: &Pubkey,
    claimer: &Pubkey,
    nonce: u64,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            Claim::SEEDS_PREFIX,
            airdrop.as_ref(),
            claimer.as_ref(),
            nonce.to_le_bytes().as_ref(),
        ],
        &psyche_solana_distributor::ID,
    )
    .0
}

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
            let sibling_index = if index.is_multiple_of(2) {
                index + 1
            } else {
                index - 1
            };
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

pub async fn process_airdrop_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_merkle_root: &MerkleHash,
    airdrop_metadata: AirdropMetadata,
    collateral_mint: &Pubkey,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );
    let accounts = AirdropCreateAccounts {
        payer: payer.pubkey(),
        authority: airdrop_authority.pubkey(),
        airdrop,
        airdrop_collateral,
        collateral_mint: *collateral_mint,
        associated_token_program: associated_token::ID,
        token_program: token::ID,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_distributor::id(),
        accounts: accounts.to_account_metas(None),
        data: AirdropCreate {
            params: AirdropCreateParams {
                id: airdrop_id,
                merkle_root: airdrop_merkle_root.clone(),
                metadata: airdrop_metadata,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(
            payer,
            instruction,
            &[airdrop_authority],
        )
        .await?;
    Ok(())
}

pub async fn process_airdrop_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_claim_freeze: Option<bool>,
    airdrop_merkle_root: Option<MerkleHash>,
    airdrop_metadata: Option<AirdropMetadata>,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let accounts = AirdropUpdateAccounts {
        authority: airdrop_authority.pubkey(),
        airdrop,
    };
    let instruction = Instruction {
        program_id: psyche_solana_distributor::id(),
        accounts: accounts.to_account_metas(None),
        data: AirdropUpdate {
            params: AirdropUpdateParams {
                claim_freeze: airdrop_claim_freeze,
                merkle_root: airdrop_merkle_root,
                metadata: airdrop_metadata,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(
            payer,
            instruction,
            &[airdrop_authority],
        )
        .await?;
    Ok(())
}

pub async fn process_airdrop_withdraw(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    receiver_collateral: &Pubkey,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );
    let accounts = AirdropWithdrawAccounts {
        authority: airdrop_authority.pubkey(),
        receiver_collateral: *receiver_collateral,
        airdrop,
        airdrop_collateral,
        collateral_mint: *collateral_mint,
        associated_token_program: associated_token::ID,
        token_program: token::ID,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_distributor::id(),
        accounts: accounts.to_account_metas(None),
        data: AirdropWithdraw {
            params: AirdropWithdrawParams { collateral_amount },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(
            payer,
            instruction,
            &[airdrop_authority],
        )
        .await?;
    Ok(())
}

pub async fn process_claim_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    airdrop_id: u64,
    allocation_nonce: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let claim = find_pda_claim(&airdrop, &claimer.pubkey(), allocation_nonce);
    let accounts = ClaimCreateAccounts {
        payer: payer.pubkey(),
        claimer: claimer.pubkey(),
        airdrop,
        claim,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_distributor::id(),
        accounts: accounts.to_account_metas(None),
        data: ClaimCreate {
            params: ClaimCreateParams {
                nonce: allocation_nonce,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[claimer])
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn process_claim_redeem(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    receiver_collateral: &Pubkey,
    airdrop_id: u64,
    allocation_nonce: u64,
    allocation_vesting: &Vesting,
    merkle_proof: &[MerkleHash],
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );
    let claim = find_pda_claim(&airdrop, &claimer.pubkey(), allocation_nonce);
    let accounts = ClaimRedeemAccounts {
        claimer: claimer.pubkey(),
        receiver_collateral: *receiver_collateral,
        airdrop,
        airdrop_collateral,
        collateral_mint: *collateral_mint,
        claim,
        token_program: token::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_distributor::id(),
        accounts: accounts.to_account_metas(None),
        data: ClaimRedeem {
            params: ClaimRedeemParams {
                nonce: allocation_nonce,
                vesting: *allocation_vesting,
                merkle_proof: merkle_proof.to_vec(),
                collateral_amount,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[claimer])
        .await?;
    Ok(())
}
