use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::system_program;
use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationReadParams {
    #[clap(long, env)]
    join_authority: Pubkey,
    #[clap(long, env)]
    authorizer: Option<Pubkey>,
}

pub async fn command_join_authorization_read_execute(
    backend: SolanaBackend,
    params: CommandJoinAuthorizationReadParams,
) -> Result<()> {
    let CommandJoinAuthorizationReadParams {
        join_authority,
        authorizer,
    } = params;

    let grantor = join_authority;
    let grantee = authorizer.unwrap_or(system_program::ID);
    let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

    println!("Authorization Grantor: {}", grantor);
    println!("Authorization Grantee: {}", grantee);

    let authorization_address =
        psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope);
    println!("Authorization Address: {}", authorization_address);

    let authorization_content = backend.get_authorization(&authorization_address).await?;
    println!("Authorization Active: {}", authorization_content.active);
    println!(
        "Authorization Delegate Count: {}",
        authorization_content.delegates.len()
    );
    for (i, authorization_delegate) in authorization_content.delegates.iter().enumerate() {
        println!(
            " - Authorization delegate #{}: {}",
            i + 1,
            authorization_delegate
        );
    }

    Ok(())
}
