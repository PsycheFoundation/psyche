use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;
use crate::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationCreateParams {
    #[clap(long, env)]
    authorizer: Pubkey,
}

pub async fn command_join_authorization_create_execute(
    backend: SolanaBackend,
    params: CommandJoinAuthorizationCreateParams,
) -> Result<()> {
    let CommandJoinAuthorizationCreateParams { authorizer } = params;

    let payer = backend.get_payer();
    let grantor = backend.get_payer();
    let grantee = authorizer;
    let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

    println!("Authorization Grantor: {}", grantor);
    println!("Authorization Grantee: {}", grantee);

    let authorization_address =
        psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope);
    println!("Authorization Address: {}", authorization_address);
    let authorization_lamports = backend.get_balance(&authorization_address).await?;
    println!("Authorization Lamports: {}", authorization_lamports);

    if authorization_lamports == 0 {
        println!(
            "Created authorization in transaction: {}",
            backend
                .send_and_retry(
                    "Authorization create",
                    &[instructions::authorizer_authorization_create(
                        &payer, &grantor, &grantee, scope,
                    )],
                    &[],
                )
                .await?
        );
    }

    let authorization_content = backend.get_authorization(&authorization_address).await?;
    println!("Authorization Active: {}", authorization_content.active);

    if !authorization_content.active {
        println!(
            "Activated authorization in transaction: {}",
            backend
                .send_and_retry(
                    "Authorization activate",
                    &[instructions::authorizer_authorization_grantor_update(
                        &grantor, &grantee, scope, true
                    )],
                    &[],
                )
                .await?
        );
    }

    Ok(())
}
