use anyhow::Result;
use psyche_solana_distributor::accounts::ClaimCreateAccounts;
use psyche_solana_distributor::find_airdrop;
use psyche_solana_distributor::find_claim;
use psyche_solana_distributor::instruction::ClaimCreate;
use psyche_solana_distributor::logic::ClaimCreateParams;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_claim_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    airdrop_index: u64,
) -> Result<()> {
    let airdrop = find_airdrop(airdrop_index);
    let claim = find_claim(&airdrop, &claimer.pubkey());

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
            params: ClaimCreateParams {},
        },
        payer,
        &[claimer],
    )
    .await?;

    Ok(())
}
