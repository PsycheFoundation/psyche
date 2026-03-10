use crate::commands::Command;
use anchor_spl::{associated_token, token};
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::utils::native_amount_to_ui_amount;
use psyche_solana_rpc::utils::ui_amount_to_native_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTreasurerTopUpRewards {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
    #[clap(long, env)]
    pub collateral_amount: f64,
}

#[async_trait]
impl Command for CommandTreasurerTopUpRewards {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
            collateral_amount,
        } = self;

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

        let collateral_mint_decimals = backend
            .get_token_mint(&treasurer_run_state.collateral_mint)
            .await?
            .decimals;
        let collateral_amount =
            ui_amount_to_native_amount(collateral_amount, collateral_mint_decimals);

        let treasurer_run_collateral_address = associated_token::get_associated_token_address(
            &treasurer_run_address,
            &treasurer_run_state.collateral_mint,
        );
        let treasurer_run_collateral_amount = backend
            .get_token_account(&treasurer_run_collateral_address)
            .await?
            .amount;
        println!(
            "Treasurer collateral amount: {}",
            native_amount_to_ui_amount(treasurer_run_collateral_amount, collateral_mint_decimals)
        );

        let user = backend.get_payer();
        println!("User: {user}");

        let user_collateral_address = associated_token::get_associated_token_address(
            &user,
            &treasurer_run_state.collateral_mint,
        );
        let user_collateral_amount = backend
            .get_token_account(&user_collateral_address)
            .await?
            .amount;
        println!(
            "User collateral amount: {}",
            native_amount_to_ui_amount(user_collateral_amount, collateral_mint_decimals)
        );

        let instruction = token::spl_token::instruction::transfer(
            &token::ID,
            &user_collateral_address,
            &treasurer_run_collateral_address,
            &user,
            &[],
            collateral_amount,
        )?;
        let signature = backend
            .send_and_retry("Top-up rewards", &[instruction], &[])
            .await?;
        println!(
            "Transfered {} collateral to treasurer in transaction: {}",
            native_amount_to_ui_amount(collateral_amount, collateral_mint_decimals),
            signature
        );

        Ok(())
    }
}
