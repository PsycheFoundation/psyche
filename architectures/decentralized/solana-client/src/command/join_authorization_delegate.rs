use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_spl::associated_token;
use anyhow::Result;
use clap::Args;
use serde_json::json;
use serde_json::to_string_pretty;

use crate::SolanaBackend;
use crate::instructions;
use crate::utils::native_amount_to_ui_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationDelegateParams {
    #[clap(long, env, value_name = "PUBKEY")]
    join_authority: Pubkey,
    #[clap(long, env)]
    delegates_clear: bool,
    #[clap(long, env, value_name = "PUBKEYS", alias = "delegate_added")]
    delegates_added: Vec<Pubkey>,
}

pub async fn command_join_authorization_delegate_execute(
    backend: SolanaBackend,
    params: CommandJoinAuthorizationDelegateParams,
) -> Result<()> {
    let CommandJoinAuthorizationDelegateParams {
        join_authority,
        delegates_clear,
        delegates_added,
    } = params;

    let payer = backend.get_payer();
    let grantor = join_authority;
    let grantee = backend.get_payer();
    let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

    println!("Authorization Grantor: {}", grantor);
    println!("Authorization Grantee: {}", grantee);
    println!(
        "Authorization Address: {}",
        psyche_solana_authorizer::find_authorization(grantor, grantee, scope)
    );

    let instruction = instructions::authorizer_authorization_grantee_update(
        &payer,
        &grantor,
        &grantee,
        scope,
        delegates_clear,
        delegates_added,
    );

    let signature = backend
        .send_and_retry(
            "Authorization create",
            &[instruction_create, instruction_activate],
            &[],
        )
        .await?;
    println!(
        "Updated authorization delegates in transaction: {}",
        signature
    );

    Ok(())
}
