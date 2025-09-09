use anyhow::Result;
use psyche_solana_mining_pool::accounts::LenderCreateAccounts;
use psyche_solana_mining_pool::find_lender;
use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::instruction::LenderCreate;
use psyche_solana_mining_pool::logic::LenderCreateParams;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_lender_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    pool_index: u64,
) -> Result<()> {
    let pool = find_pool(pool_index);
    let lender = find_lender(&pool, &user.pubkey());

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_mining_pool::id(),
        LenderCreateAccounts {
            payer: payer.pubkey(),
            user: user.pubkey(),
            pool,
            lender,
            system_program: system_program::ID,
        },
        LenderCreate {
            params: LenderCreateParams {},
        },
        payer,
        &[user],
    )
    .await?;

    Ok(())
}
