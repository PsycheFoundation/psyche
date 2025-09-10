use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_mining_pool::accounts::LenderClaimAccounts;
use psyche_solana_mining_pool::find_lender;
use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::instruction::LenderClaim;
use psyche_solana_mining_pool::logic::LenderClaimParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

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

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        LenderClaimAccounts {
            user: user.pubkey(),
            user_redeemable: *user_redeemable,
            pool,
            pool_redeemable,
            redeemable_mint: *redeemable_mint,
            lender,
            token_program: token::ID,
        },
        LenderClaim {
            params: LenderClaimParams { redeemable_amount },
        },
        payer,
        &[user],
    )
    .await?;

    Ok(())
}
