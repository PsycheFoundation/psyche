use psyche_solana_mining_pool::accounts::PoolClaimableAccounts;
use psyche_solana_mining_pool::instruction::PoolClaimable;
use psyche_solana_mining_pool::logic::PoolClaimableParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_anchor::ToolboxAnchorError;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pda_pool::find_pda_pool;

pub async fn process_pool_claimable(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    redeemable_mint: &Pubkey,
) -> Result<(), ToolboxAnchorError> {
    let pool = find_pda_pool(pool_index);

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        PoolClaimableAccounts {
            authority: pool_authority.pubkey(),
            redeemable_mint: *redeemable_mint,
            pool,
        },
        PoolClaimable { params: PoolClaimableParams {} },
        payer,
        &[pool_authority],
    )
    .await?;

    Ok(())
}
