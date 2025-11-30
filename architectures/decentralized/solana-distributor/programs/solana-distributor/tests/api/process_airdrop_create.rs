use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropCreateAccounts;
use psyche_solana_distributor::find_airdrop;
use psyche_solana_distributor::instruction::AirdropCreate;
use psyche_solana_distributor::logic::AirdropCreateParams;
use psyche_solana_distributor::state::Airdrop;
use psyche_solana_distributor::state::AirdropMerkleHash;
use psyche_solana_distributor::state::AirdropMetadata;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_airdrop_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_index: u64,
    airdrop_authority: &Keypair,
    airdrop_merkle_root: AirdropMerkleHash,
    airdrop_metadata: AirdropMetadata,
    collateral_mint: &Pubkey,
) -> Result<()> {
    let airdrop = find_airdrop(airdrop_index);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropCreateAccounts {
            payer: payer.pubkey(),
            authority: airdrop_authority.pubkey(),
            airdrop,
            airdrop_collateral,
            collateral_mint: *collateral_mint,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        },
        AirdropCreate {
            params: AirdropCreateParams {
                index: airdrop_index,
                merkle_root: airdrop_merkle_root,
                metadata: airdrop_metadata,
            },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;

    let airdrop_data_after = ToolboxAnchor::get_account_data_deserialized::<
        Airdrop,
    >(endpoint, &airdrop)
    .await?
    .unwrap();

    assert_eq!(airdrop_data_after.index, airdrop_index);
    assert_eq!(airdrop_data_after.authority, airdrop_authority.pubkey());

    assert_eq!(airdrop_data_after.collateral_mint, *collateral_mint);
    assert_eq!(airdrop_data_after.total_claimed_collateral_amount, 0);

    assert_eq!(airdrop_data_after.freeze, false);
    assert_eq!(airdrop_data_after.merkle_root, airdrop_merkle_root);
    assert_eq!(airdrop_data_after.metadata, airdrop_metadata);

    Ok(())
}
