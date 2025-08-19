use anchor_client::solana_sdk::native_token::lamports_to_sol;
use anchor_spl::associated_token;
use anyhow::Result;
use clap::Args;
use serde_json::json;
use serde_json::{to_string, Map};

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCreateRunParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    treasurer_collateral_mint: Option<String>,
    #[clap(long)]
    join_authority: Option<String>,
}

pub async fn command_create_run_execute(
    backend: SolanaBackend,
    params: CommandCreateRunParams,
) -> Result<()> {
    let CommandCreateRunParams {
        run_id,
        treasurer_index,
        treasurer_collateral_mint,
        join_authority,
    } = params;

    if treasurer_index.is_some() && treasurer_collateral_mint.is_none() {
        bail!(
            "treasurer_index is set, but treasurer_collateral_mint is not. Please provide a collateral mint address."
        );
    }

    let treasurer_index_and_collateral_mint =
        treasurer_collateral_mint.map(|treasurer_collateral_mint| {
            let treasurer_index =
                SolanaBackend::compute_deterministic_treasurer_index(&run_id, treasurer_index);
            let treasurer_collateral_mint = treasurer_collateral_mint
                .parse::<Pubkey>()
                .expect("Invalid collateral mint address");
            (treasurer_index, treasurer_collateral_mint)
        });

    let created = backend
        .create_run(
            &run_id,
            treasurer_index_and_collateral_mint,
            join_authority.map(|address| Pubkey::from_str(&address).unwrap()),
        )
        .await?;

    println!(
        "Created run {} with transaction: {:?}",
        run_id, created.signature,
    );
    println!("Instance account: {}", created.instance);
    println!("Coordinator account: {}", created.account);

    let locked = backend.get_balance(&created.account).await?;
    println!("Locked for storage: {:.9} SOL", lamports_to_sol(locked));

    Ok(())
}
