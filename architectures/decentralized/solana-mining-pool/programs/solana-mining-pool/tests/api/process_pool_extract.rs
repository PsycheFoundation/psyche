use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_mining_pool::accounts::PoolExtractAccounts;
use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::instruction::PoolExtract;
use psyche_solana_mining_pool::logic::PoolExtractParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

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

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        PoolExtractAccounts {
            authority: pool_authority.pubkey(),
            authority_collateral: *pool_authority_collateral,
            pool,
            pool_collateral,
            token_program: token::ID,
        },
        PoolExtract {
            params: PoolExtractParams { collateral_amount },
        },
        payer,
        &[pool_authority],
    )
    .await?;

    Ok(())
}
