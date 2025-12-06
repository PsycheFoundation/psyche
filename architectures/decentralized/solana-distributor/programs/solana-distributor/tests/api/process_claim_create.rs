use anyhow::Result;
use psyche_solana_distributor::accounts::ClaimCreateAccounts;
use psyche_solana_distributor::instruction::ClaimCreate;
use psyche_solana_distributor::logic::ClaimCreateParams;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pdas::find_pda_airdrop;
use crate::api::find_pdas::find_pda_claim;

pub async fn process_claim_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    airdrop_id: u64,
    allocation_nonce: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let claim = find_pda_claim(&airdrop, &claimer.pubkey(), allocation_nonce);

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        ClaimCreateAccounts {
            payer: payer.pubkey(),
            claimer: claimer.pubkey(),
            airdrop,
            claim,
            system_program: system_program::ID,
        },
        ClaimCreate {
            params: ClaimCreateParams {
                nonce: allocation_nonce,
            },
        },
        payer,
        &[claimer],
    )
    .await?;

    Ok(())
}
