use crate::commands::Command;
use crate::commands::authorization::Authorization;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationCreate {
    /// Authorization type: either a pubkey address or "permissionless" (maps to system program ID)
    #[clap(long, env)]
    pub authorization: Authorization,
}

#[async_trait]
impl Command for CommandJoinAuthorizationCreate {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let authorizer = self.authorization.to_pubkey();

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
}
