use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::system_program;
use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;
use crate::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationCreateParams {
    #[clap(long, env)]
    authorizer: Option<Pubkey>,
}

pub async fn command_join_authorization_create_execute(
    backend: SolanaBackend,
    params: CommandJoinAuthorizationCreateParams,
) -> Result<()> {
    let CommandJoinAuthorizationCreateParams { authorizer } = params;

    let payer = backend.get_payer();
    let grantor = backend.get_payer();
    let grantee = authorizer.unwrap_or(system_program::ID);
    let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

    println!("Authorization Grantor: {}", grantor);
    println!("Authorization Grantee: {}", grantee);
    println!(
        "Authorization Address: {}",
        psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope)
    );

    let instruction_create =
        instructions::authorizer_authorization_create(&payer, &grantor, &grantee, scope);
    let instruction_activate =
        instructions::authorizer_authorization_grantor_update(&grantor, &grantee, scope, true);

    let signature = backend
        .send_and_retry(
            "Authorization create",
            &[instruction_create, instruction_activate],
            &[],
        )
        .await?;
    println!(
        "Created and activated authorization in transaction: {}",
        signature
    );

    Ok(())
}
