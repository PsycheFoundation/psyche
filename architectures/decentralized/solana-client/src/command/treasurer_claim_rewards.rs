use anchor_spl::associated_token;
use anyhow::{Context, Result, bail};
use clap::Args;

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTreasurerClaimRewardsParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    max_claimed_points: Option<u64>,
}

pub async fn command_treasurer_claim_rewards_execute(
    backend: SolanaBackend,
    params: CommandTreasurerClaimRewardsParams,
) -> Result<()> {
    let CommandTreasurerClaimRewardsParams {
        run_id,
        treasurer_index,
        max_claimed_points,
    } = params;

    let treasurer_index = backend
        .resolve_treasurer_index(&run_id, treasurer_index)
        .await?
        .context("Failed to resolve treasurer")?;
    println!("Found treasurer at index: 0x{treasurer_index:08x?}");

    let treasurer_run_address = psyche_solana_treasurer::find_run(treasurer_index);
    let treasurer_run_state = backend.get_treasurer_run(&treasurer_run_address).await?;
    println!(
        "Treasurer collateral mint: {}",
        treasurer_run_state.collateral_mint
    );

    let treasurer_run_collateral_address = associated_token::get_associated_token_address(
        &treasurer_run_address,
        &treasurer_run_state.collateral_mint,
    );
    let treasurer_run_collateral_amount = backend
        .get_token_amount(&treasurer_run_collateral_address)
        .await?;
    println!("Treasurer collateral amount: {treasurer_run_collateral_amount}");

    let user = backend.get_payer();
    println!("User: {user}");

    let user_collateral_address =
        associated_token::get_associated_token_address(&user, &treasurer_run_state.collateral_mint);
    if backend.get_balance(&user_collateral_address).await? == 0 {
        let signature = backend
            .spl_associated_token_create(&treasurer_run_state.collateral_mint, &user)
            .await?;
        println!("Created associated token account for user during transaction: {signature}");
    }

    let user_collateral_amount = backend.get_token_amount(&user_collateral_address).await?;
    println!("User collateral amount: {user_collateral_amount}");

    let treasurer_participant_address =
        psyche_solana_treasurer::find_participant(&treasurer_run_address, &user);
    if backend.get_balance(&treasurer_participant_address).await? == 0 {
        let signature = backend
            .treasurer_participant_create(treasurer_index)
            .await?;
        println!("Created the participant claim during transaction: {signature}");
    }

    let mut client_earned_points = 0;
    let coordinator_account_state = backend
        .get_coordinator_account(&treasurer_run_state.coordinator_account)
        .await?;
    for client in coordinator_account_state.state.clients_state.clients {
        if user == client.id.signer {
            client_earned_points = client.earned;
            break;
        }
    }
    println!("Total earned points: {client_earned_points}");

    let treasurer_participiant_state = backend
        .get_treasurer_participant(&treasurer_participant_address)
        .await?;
    println!(
        "Already claimed earned points: {}",
        treasurer_participiant_state.claimed_earned_points
    );

    let claimable_earned_points =
        client_earned_points - treasurer_participiant_state.claimed_earned_points;
    println!("Claimable earned points: {claimable_earned_points}");

    let claim_earned_points = std::cmp::min(
        claimable_earned_points,
        max_claimed_points.unwrap_or(u64::MAX),
    );
    if claim_earned_points > treasurer_run_collateral_amount {
        return bail!(
            "Claimed points ({claim_earned_points}) exceed funded collateral amount ({treasurer_run_collateral_amount}), specify a smaller value for --max-claimed-points or wait for more funding to be added to the run"
        );
    }

    let signature = backend
        .treasurer_participant_claim(
            treasurer_index,
            &treasurer_run_state.collateral_mint,
            &treasurer_run_state.coordinator_account,
            claim_earned_points,
        )
        .await?;
    println!("Claimed {claim_earned_points} earned points in transaction: {signature}");

    Ok(())
}
