use anyhow::Result;
use clap::Args;

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandUpdateClientVersionParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    new_version: String,
}

pub async fn command_update_client_version_execute(
    backend: SolanaBackend,
    params: CommandUpdateClientVersionParams,
) -> Result<()> {
    let CommandUpdateClientVersionParams {
        run_id,
        new_version,
    } = params;
    let main_authority = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;
    let instruction = instructions::coordinator_update_client_version(
        &run_id,
        &coordinator_account,
        &main_authority,
        &new_version,
    );

    let signature = backend
        .send_and_retry("Update client version", &[instruction], &[])
        .await?;

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
