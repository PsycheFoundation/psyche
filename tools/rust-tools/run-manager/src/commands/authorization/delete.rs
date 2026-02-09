use crate::commands::Command;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::system_program;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::instructions;
use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationDelete {
    #[clap(long, env, conflicts_with = "permissionless")]
    pub authorizer: Option<Pubkey>,

    /// Delete a permissionless authorization (uses system program ID)
    #[clap(long, conflicts_with = "authorizer")]
    pub permissionless: bool,
}

#[async_trait]
impl Command for CommandJoinAuthorizationDelete {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            authorizer,
            permissionless,
        } = self;

        let authorizer = if permissionless {
            system_program::ID
        } else {
            authorizer.ok_or_else(|| {
                anyhow::anyhow!("Either --authorizer or --permissionless must be provided")
            })?
        };

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
}
