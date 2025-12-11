use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_mining_pool::accounts::LenderClaimAccounts;
use psyche_solana_mining_pool::accounts::LenderCreateAccounts;
use psyche_solana_mining_pool::accounts::LenderDepositAccounts;
use psyche_solana_mining_pool::accounts::PoolClaimableAccounts;
use psyche_solana_mining_pool::accounts::PoolCreateAccounts;
use psyche_solana_mining_pool::accounts::PoolExtractAccounts;
use psyche_solana_mining_pool::accounts::PoolUpdateAccounts;
use psyche_solana_mining_pool::find_lender;
use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::instruction::LenderClaim;
use psyche_solana_mining_pool::instruction::LenderCreate;
use psyche_solana_mining_pool::instruction::LenderDeposit;
use psyche_solana_mining_pool::instruction::PoolClaimable;
use psyche_solana_mining_pool::instruction::PoolCreate;
use psyche_solana_mining_pool::instruction::PoolExtract;
use psyche_solana_mining_pool::instruction::PoolUpdate;
use psyche_solana_mining_pool::logic::LenderClaimParams;
use psyche_solana_mining_pool::logic::LenderCreateParams;
use psyche_solana_mining_pool::logic::LenderDepositParams;
use psyche_solana_mining_pool::logic::PoolClaimableParams;
use psyche_solana_mining_pool::logic::PoolCreateParams;
use psyche_solana_mining_pool::logic::PoolExtractParams;
use psyche_solana_mining_pool::logic::PoolUpdateParams;
use psyche_solana_mining_pool::state::PoolMetadata;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_pool_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    pool_metadata: PoolMetadata,
    collateral_mint: &Pubkey,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let pool_collateral =
        associated_token::get_associated_token_address(&pool, collateral_mint);
    let accounts = PoolCreateAccounts {
        payer: payer.pubkey(),
        authority: pool_authority.pubkey(),
        pool,
        pool_collateral,
        collateral_mint: *collateral_mint,
        associated_token_program: associated_token::ID,
        token_program: token::ID,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: PoolCreate {
            params: PoolCreateParams {
                index: pool_index,
                metadata: pool_metadata,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[pool_authority])
        .await?;
    Ok(())
}

pub async fn process_pool_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    pool_max_deposit_collateral_amount: Option<u64>,
    pool_freeze: Option<bool>,
    pool_metadata: Option<PoolMetadata>,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let accounts = PoolUpdateAccounts {
        authority: pool_authority.pubkey(),
        pool,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: PoolUpdate {
            params: PoolUpdateParams {
                max_deposit_collateral_amount:
                    pool_max_deposit_collateral_amount,
                freeze: pool_freeze,
                metadata: pool_metadata,
            },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[pool_authority])
        .await?;
    Ok(())
}

pub async fn process_pool_extract(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    pool_authority_collateral: &Pubkey,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let pool_collateral =
        associated_token::get_associated_token_address(&pool, collateral_mint);
    let accounts = PoolExtractAccounts {
        authority: pool_authority.pubkey(),
        authority_collateral: *pool_authority_collateral,
        pool,
        pool_collateral,
        token_program: token::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: PoolExtract {
            params: PoolExtractParams { collateral_amount },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[pool_authority])
        .await?;
    Ok(())
}

pub async fn process_pool_claimable(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    redeemable_mint: &Pubkey,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let accounts = PoolClaimableAccounts {
        authority: pool_authority.pubkey(),
        redeemable_mint: *redeemable_mint,
        pool,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: PoolClaimable {
            params: PoolClaimableParams {},
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[pool_authority])
        .await?;
    Ok(())
}

pub async fn process_lender_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    pool_index: u64,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let lender = find_lender(&pool, &user.pubkey());
    let accounts = LenderCreateAccounts {
        payer: payer.pubkey(),
        user: user.pubkey(),
        pool,
        lender,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: LenderCreate {
            params: LenderCreateParams {},
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}

pub async fn process_lender_deposit(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    user_collateral: &Pubkey,
    pool_index: u64,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let pool_collateral =
        associated_token::get_associated_token_address(&pool, collateral_mint);
    let lender = find_lender(&pool, &user.pubkey());
    let accounts = LenderDepositAccounts {
        user: user.pubkey(),
        user_collateral: *user_collateral,
        pool,
        pool_collateral,
        lender,
        token_program: token::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: LenderDeposit {
            params: LenderDepositParams { collateral_amount },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}

pub async fn process_lender_claim(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    user_redeemable: &Pubkey,
    pool_index: u64,
    redeemable_mint: &Pubkey,
    redeemable_amount: u64,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let pool_redeemable =
        associated_token::get_associated_token_address(&pool, redeemable_mint);
    let lender = find_lender(&pool, &user.pubkey());
    let accounts = LenderClaimAccounts {
        user: user.pubkey(),
        user_redeemable: *user_redeemable,
        pool,
        pool_redeemable,
        redeemable_mint: *redeemable_mint,
        lender,
        token_program: token::ID,
    };
    let instruction = Instruction {
        program_id: psyche_solana_mining_pool::id(),
        accounts: accounts.to_account_metas(None),
        data: LenderClaim {
            params: LenderClaimParams { redeemable_amount },
        }
        .data(),
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}
