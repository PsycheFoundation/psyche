use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;
use crate::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJoinAuthorizationDelegateParams {
    #[clap(long, env)]
    join_authority: Pubkey,
    #[clap(long, env, default_value_t = false)]
    delegates_clear: bool,
    #[clap(long, env, alias = "delegate-added", num_args = 0.., value_name = "PUBKEY(S)")]
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
        psyche_solana_authorizer::find_authorization(&grantor, &grantee, scope)
    );

    println!("Delegates cleared: {}", delegates_clear);
    println!("Delegates added count: {}", delegates_added.len());
    for delegate_added in &delegates_added {
        println!("- Delegate added: {}", delegate_added);
    }

    let instruction = instructions::authorizer_authorization_grantee_update(
        &payer,
        &grantor,
        &grantee,
        scope,
        delegates_clear,
        delegates_added,
    );

    let signature = backend
        .send_and_retry("Authorization set delegates", &[instruction], &[])
        .await?;
    println!(
        "Updated authorization delegates in transaction: {}",
        signature
    );

    Ok(())
}
