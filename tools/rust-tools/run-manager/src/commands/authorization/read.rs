use crate::commands::Command;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::system_program;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationRead {
    #[clap(long, env)]
    pub join_authority: Pubkey,
    #[clap(long, env, conflicts_with = "permissionless")]
    pub authorizer: Option<Pubkey>,
    /// Read permissionless authorization (uses system program ID)
    #[clap(long, conflicts_with = "authorizer")]
    pub permissionless: bool,
}

#[async_trait]
impl Command for CommandJoinAuthorizationRead {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            join_authority,
            authorizer,
            permissionless,
        } = self;

        let grantor = join_authority;
        let grantee = if permissionless {
            system_program::ID
        } else {
            authorizer.unwrap_or(system_program::ID)
        };
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
}
