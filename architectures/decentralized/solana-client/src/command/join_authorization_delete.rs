use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;
use crate::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationDeleteParams {
    #[clap(long, env)]
    authorizer: Pubkey,
}

pub async fn command_join_authorization_delete_execute(
    backend: SolanaBackend,
    params: CommandJoinAuthorizationDeleteParams,
) -> Result<()> {
    let CommandJoinAuthorizationDeleteParams { authorizer } = params;

    let grantor = backend.get_payer();
    let grantee = authorizer;
    let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

    println!("Authorization Grantor: {}", grantor);
    println!("Authorization Grantee: {}", grantee);

    let authorization_address =
        psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope);
    println!("Authorization Address: {}", authorization_address);

    let authorization_content = backend.get_authorization(&authorization_address).await?;
    println!("Authorization Active: {}", authorization_content.active);

    if authorization_content.active {
        println!(
            "Deactivated authorization in transaction: {}",
            backend
                .send_and_retry(
                    "Authorization deactivate",
                    &[instructions::authorizer_authorization_grantor_update(
                        &grantor, &grantee, scope, false,
                    )],
                    &[],
                )
                .await?
        );
    }

    Ok(())
}
