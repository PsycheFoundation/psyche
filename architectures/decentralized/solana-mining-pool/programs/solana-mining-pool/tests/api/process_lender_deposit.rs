use anchor_spl::associated_token;
use anchor_spl::token;
use psyche_solana_mining_pool::accounts::LenderDepositAccounts;
use psyche_solana_mining_pool::instruction::LenderDeposit;
use psyche_solana_mining_pool::logic::LenderDepositParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_anchor::ToolboxAnchorError;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pda_lender::find_pda_lender;
use crate::api::find_pda_pool::find_pda_pool;

pub async fn process_lender_deposit(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    user_collateral: &Pubkey,
    pool_index: u64,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<(), ToolboxAnchorError> {
    let pool = find_pda_pool(pool_index);
    let pool_collateral =
        associated_token::get_associated_token_address(&pool, collateral_mint);

    let lender = find_pda_lender(&pool, &user.pubkey());

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        LenderDepositAccounts {
            user: user.pubkey(),
            user_collateral: *user_collateral,
            pool,
            pool_collateral,
            lender,
            token_program: token::ID,
        },
        LenderDeposit { params: LenderDepositParams { collateral_amount } },
        payer,
        &[user],
    )
    .await?;

    Ok(())
}
