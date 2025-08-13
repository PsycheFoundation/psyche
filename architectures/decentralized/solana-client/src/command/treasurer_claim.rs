use anyhow::{bail, Context, Result};
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
    max_collateral_amount: Option<u64>,
}

pub async fn command_treasurer_claim_run(
    backend: SolanaBackend,
    params: CommandTreasurerClaimParams,
) -> Result<()> {
    let treasurer_index = backend
        .resolve_treasurer_index(params.run_id, params.treasurer_index)
        .await?
        .context("Failed to resolve treasurer")?;

    let treasurer_run_address = psyche_solana_treasurer::find_run(treasurer_index);
    let treasurer_run_state = backend.get_treasurer_run(&treasurer_run_address).await?;
    let treasurer_run_collateral_address =
        spl_associated_token_account::get_associated_token_address(
            &treasurer_run_address,
            &treasurer_run_state.collateral_mint,
        );
    let treasurer_run_collateral_amount = backend
        .get_token_amount(&treasurer_run_collateral_address)
        .await?;

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

    let earned_collateral_amount = signer_earned_points;
    let claimable_collateral_amount =
        std::min(treasurer_run_collateral_amount, earned_collateral_amount);

    let claimed_collateral_amount = std::min(
        claimable_collateral_amount,
        params.max_collateral_amount.unwrap_or(u64::MAX),
    );

    let treasurer_participant_address =
        psyche_solana_treasurer::find_participant(&treasurer_run_address, &signer);

    if backend.get_balance(treasurer_participant_address).await? == 0 {
        backend
            .treasurer_participant_create(treasurer_index)
            .await?;
    }

    backend
        .treasurer_participant_create(
            treasurer_index,
            treasurer_run_state.collateral_mint,
            treasurer_run_state.coordinator_account,
            claimable_collateral_amount,
        )
        .await?;

    Ok(())
}
