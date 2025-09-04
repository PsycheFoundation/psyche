use anchor_client::solana_sdk::native_token::lamports_to_sol;
use anyhow::Result;
use clap::Args;

use crate::instructions;
use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCloseRunParams {
    #[clap(short, long, env)]
    run_id: String,
}

pub async fn command_close_run_execute(
    backend: SolanaBackend,
    params: CommandCloseRunParams,
) -> Result<()> {
    let CommandCloseRunParams { run_id } = params;

    let payer = backend.get_payer();

    let before_lamports = backend.get_balance(&payer).await?;

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;

    let instruction =
        instructions::coordinator_close_run(&coordinator_instance, &coordinator_account, &payer);
    let signature = backend
        .send_and_retry("Close run", &[instruction], &[])
        .await?;
    println!("Closed run {run_id} with transaction {signature}");

    let after_lamports = backend.get_balance(&payer).await?;

    let recovered_lamports = after_lamports - before_lamports;
    let recovered_sols = lamports_to_sol(recovered_lamports);

    println!("Recovered {recovered_sols:.9} SOL");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
