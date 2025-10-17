use anyhow::Result;
use clap::Args;

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandUpdateVersionTagParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    new_tag: String,
}

pub async fn command_update_version_tag_execute(
    backend: SolanaBackend,
    params: CommandUpdateVersionTagParams,
) -> Result<()> {
    let CommandUpdateVersionTagParams { run_id, new_tag } = params;
    let main_authority = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;
    let instruction = instructions::coordinator_update_version_tag(
        &run_id,
        &coordinator_account,
        &main_authority,
        &new_tag,
    );

    let signature = backend
        .send_and_retry("Update version tag", &[instruction], &[])
        .await?;
    // println!("Set pause state to {paused} on run {run_id} with transaction {signature}");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
