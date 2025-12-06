use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropUpdateAccounts;
use psyche_solana_distributor::instruction::AirdropUpdate;
use psyche_solana_distributor::logic::AirdropUpdateParams;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::MerkleHash;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pdas::find_pda_airdrop;

pub async fn process_airdrop_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_freeze: Option<bool>,
    airdrop_merkle_root: Option<MerkleHash>,
    airdrop_metadata: Option<AirdropMetadata>,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);

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
