use crate::commands::Command;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationDelegate {
    #[clap(long, env)]
    pub join_authority: Pubkey,
    #[clap(long, env, default_value_t = false)]
    pub delegates_clear: bool,
    #[clap(long, env, alias = "delegate-added", num_args = 0.., value_name = "PUBKEY(S)")]
    pub delegates_added: Vec<Pubkey>,
}

#[async_trait]
impl Command for CommandJoinAuthorizationDelegate {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            join_authority,
            delegates_clear,
            delegates_added,
        } = self;

        let payer = backend.get_payer();
        let grantor = join_authority;
        let grantee = backend.get_payer();
        let scope = psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

        println!("Authorization Grantor: {}", grantor);
        println!("Authorization Grantee: {}", grantee);

        println!(
            "Authorization Address: {}",
            psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope)
        );

        println!("Delegates cleared: {}", delegates_clear);
        println!("Delegates added count: {}", delegates_added.len());
        for delegate_added in &delegates_added {
            println!("- Delegate added: {}", delegate_added);
        }

        println!(
            "Updated authorization delegates in transaction: {}",
            backend
                .send_and_retry(
                    "Authorization set delegates",
                    &[instructions::authorizer_authorization_grantee_update(
                        &payer,
                        &grantor,
                        &grantee,
                        scope,
                        delegates_clear,
                        delegates_added,
                    )],
                    &[]
                )
                .await?
        );

        Ok(())
    }
}
