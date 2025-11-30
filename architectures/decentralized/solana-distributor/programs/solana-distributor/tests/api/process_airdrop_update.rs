use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropUpdateAccounts;
use psyche_solana_distributor::find_airdrop;
use psyche_solana_distributor::instruction::AirdropUpdate;
use psyche_solana_distributor::logic::AirdropUpdateParams;
use psyche_solana_distributor::state::{AirdropMerkleHash, AirdropMetadata};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_airdrop_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_index: u64,
    airdrop_authority: &Keypair,
    airdrop_freeze: Option<bool>,
    airdrop_merkle_root: Option<AirdropMerkleHash>,
    airdrop_metadata: Option<AirdropMetadata>,
) -> Result<()> {
    let airdrop = find_airdrop(airdrop_index);

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropUpdateAccounts {
            authority: airdrop_authority.pubkey(),
            airdrop,
        },
        AirdropUpdate {
            params: AirdropUpdateParams {
                freeze: airdrop_freeze,
                merkle_root: airdrop_merkle_root,
                metadata: airdrop_metadata,
            },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;

    Ok(())
}
