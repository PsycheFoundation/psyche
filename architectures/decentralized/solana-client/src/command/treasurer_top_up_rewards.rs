use anchor_spl::{associated_token, token};
use anyhow::{Context, Result};
use clap::Args;

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTreasurerTopUpRewardsParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    collateral_amount: u64,
}

pub async fn command_treasurer_top_up_rewards_execute(
    backend: SolanaBackend,
    params: CommandTreasurerTopUpRewardsParams,
) -> Result<()> {
    let CommandTreasurerTopUpRewardsParams {
        run_id,
        treasurer_index,
        collateral_amount,
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
    let user_collateral_amount = backend.get_token_amount(&user_collateral_address).await?;
    println!("User collateral amount: {user_collateral_amount}");

    let instruction = token::spl_token::instruction::transfer(
        &token::ID,
        &user_collateral_address,
        &treasurer_run_collateral_address,
        &user,
        &[],
        collateral_amount,
    )?;
    let signature = backend.process(&[instruction], &[]).await?;
    println!("Transfered {collateral_amount} collateral to treasurer in transaction: {signature}");

    Ok(())
}
