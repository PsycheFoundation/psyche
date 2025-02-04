use psyche_solana_mining_pool::accounts::PoolUpdateAccounts;
use psyche_solana_mining_pool::instruction::PoolUpdate;
use psyche_solana_mining_pool::logic::PoolUpdateParams;
use psyche_solana_mining_pool::state::PoolMetadata;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_anchor::ToolboxAnchorError;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pda_pool::find_pda_pool;

pub async fn process_pool_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    pool_index: u64,
    pool_authority: &Keypair,
    pool_max_deposit_collateral_amount: Option<u64>,
    pool_metadata: Option<PoolMetadata>,
) -> Result<(), ToolboxAnchorError> {
    let pool = find_pda_pool(pool_index);

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        PoolUpdateAccounts { authority: pool_authority.pubkey(), pool },
        PoolUpdate {
            params: PoolUpdateParams {
                max_deposit_collateral_amount:
                    pool_max_deposit_collateral_amount,
                metadata: pool_metadata,
            },
        },
        payer,
        &[pool_authority],
    )
    .await?;

    Ok(())
}
