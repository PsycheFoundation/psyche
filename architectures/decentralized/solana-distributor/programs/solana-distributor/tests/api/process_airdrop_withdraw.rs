use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropWithdrawAccounts;
use psyche_solana_distributor::instruction::AirdropWithdraw;
use psyche_solana_distributor::logic::AirdropWithdrawParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pdas::find_pda_airdrop;

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

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropWithdrawAccounts {
            authority: airdrop_authority.pubkey(),
            receiver_collateral: *receiver_collateral,
            airdrop,
            airdrop_collateral,
            collateral_mint: *collateral_mint,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        },
        AirdropWithdraw {
            params: AirdropWithdrawParams { collateral_amount },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;

    Ok(())
}
