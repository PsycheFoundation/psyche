use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_mining_pool::accounts::PoolCreateAccounts;
use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::instruction::PoolCreate;
use psyche_solana_mining_pool::logic::PoolCreateParams;
use psyche_solana_mining_pool::state::Pool;
use psyche_solana_mining_pool::state::PoolMetadata;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
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

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        PoolCreateAccounts {
            payer: payer.pubkey(),
            authority: pool_authority.pubkey(),
            pool,
            pool_collateral,
            collateral_mint: *collateral_mint,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        },
        PoolCreate {
            params: PoolCreateParams {
                index: pool_index,
                metadata: pool_metadata,
            },
        },
        payer,
        &[pool_authority],
    )
    .await?;

    let pool_data_after =
        ToolboxAnchor::get_account_data_deserialized::<Pool>(endpoint, &pool)
            .await?
            .unwrap();

    assert_eq!(pool_data_after.index, pool_index);
    assert_eq!(pool_data_after.authority, pool_authority.pubkey());

    assert_eq!(pool_data_after.collateral_mint, *collateral_mint);
    assert_eq!(pool_data_after.total_deposited_collateral_amount, 0);
    assert_eq!(pool_data_after.total_extracted_collateral_amount, 0);

    assert_eq!(pool_data_after.claiming_enabled, false);
    assert_eq!(pool_data_after.redeemable_mint, Pubkey::default());
    assert_eq!(pool_data_after.total_claimed_redeemable_amount, 0);

    Ok(())
}
