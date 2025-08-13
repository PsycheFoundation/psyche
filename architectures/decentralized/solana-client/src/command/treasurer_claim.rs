use anyhow::{Context, Result};
use clap::Args;

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTreasurerClaimParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    max_claimed_points: Option<u64>,
}

pub async fn command_treasurer_claim_run(
    backend: SolanaBackend,
    params: CommandTreasurerClaimParams,
) -> Result<()> {
    let treasurer_index = backend
        .resolve_treasurer_index(&params.run_id, params.treasurer_index)
        .await?
        .context("Failed to resolve treasurer")?;

    let treasurer_run_address = psyche_solana_treasurer::find_run(treasurer_index);
    let treasurer_run_state = backend.get_treasurer_run(&treasurer_run_address).await?;

    let coordinator_account_state = backend
        .get_coordinator_account(&treasurer_run_state.coordinator_account)
        .await?;

    let mut signer_earned_points = 0;
    let signer = backend.get_payer();
    for client in coordinator_account_state.state.clients_state.clients {
        if signer == client.id.signer {
            signer_earned_points = client.earned;
            break;
        }
    }

    let treasurer_participant_address =
        psyche_solana_treasurer::find_participant(&treasurer_run_address, &signer);
    if backend.get_balance(&treasurer_participant_address).await? == 0 {
        backend
            .treasurer_participant_create(treasurer_index)
            .await?;
    }

    let claim_earned_points = std::cmp::min(
        signer_earned_points,
        params.max_claimed_points.unwrap_or(u64::MAX),
    );
    backend
        .treasurer_participant_claim(
            treasurer_index,
            &treasurer_run_state.collateral_mint,
            &treasurer_run_state.coordinator_account,
            claim_earned_points,
        )
        .await?;

    Ok(())
}
