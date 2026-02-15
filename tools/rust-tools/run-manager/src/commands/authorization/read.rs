use crate::commands::Command;
use crate::commands::authorization::Authorization;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationRead {
    #[clap(long, env)]
    pub join_authority: Pubkey,
    /// Authorization type: either a pubkey address or "permissionless" (maps to system program ID)
    #[clap(long, env)]
    pub authorization: Authorization,
}

#[async_trait]
impl Command for CommandJoinAuthorizationRead {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let grantor = self.join_authority;
        let grantee = self.authorization.to_pubkey();
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
