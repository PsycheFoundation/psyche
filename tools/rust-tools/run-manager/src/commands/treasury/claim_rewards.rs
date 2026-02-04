use crate::commands::Command;
use anchor_spl::{associated_token, token};
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::{SolanaBackend, instructions};
use psyche_solana_rpc::utils::native_amount_to_ui_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTreasurerClaimRewards {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
}

#[async_trait]
impl Command for CommandTreasurerClaimRewards {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
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
        if backend.get_balance(&user_collateral_address).await? == 0 {
            let instruction = associated_token::spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &backend.get_payer(),
            &user,
            &treasurer_run_state.collateral_mint,
            &token::ID,
        );
            let signature = backend
                .send_and_retry("Create user ATA", &[instruction], &[])
                .await?;
            println!("Created associated token account for user during transaction: {signature}");
        }

        let user_collateral_amount = backend
            .get_token_account(&user_collateral_address)
            .await?
            .amount;
        println!(
            "User collateral amount: {}",
            native_amount_to_ui_amount(user_collateral_amount, collateral_mint_decimals)
        );

        let treasurer_participant_address =
            psyche_solana_treasurer::find_participant(&treasurer_run_address, &user);
        if backend.get_balance(&treasurer_participant_address).await? == 0 {
            let instruction = instructions::treasurer_participant_create(
                &backend.get_payer(),
                treasurer_index,
                &user,
            );
            let participant_create_signature = backend
                .send_and_retry("Create participant PDA", &[instruction], &[])
                .await?;
            println!(
                "Created the participant claim during transaction: {participant_create_signature}"
            );
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

        // 1:1 mapping between earned points and collateral amount
        let claimable_collateral_amount = claimable_earned_points;
        println!(
            "Claimable collateral amount: {}",
            native_amount_to_ui_amount(claimable_collateral_amount, collateral_mint_decimals)
        );

        let claim_collateral_amount =
            std::cmp::min(claimable_collateral_amount, treasurer_run_collateral_amount);
        println!(
            "Claim collateral amount: {}",
            native_amount_to_ui_amount(claim_collateral_amount, collateral_mint_decimals)
        );

        // 1:1 mapping between earned points and collateral amount
        let claim_earned_points = claim_collateral_amount;

        let instruction = instructions::treasurer_participant_claim(
            treasurer_index,
            &treasurer_run_state.collateral_mint,
            &treasurer_run_state.coordinator_account,
            &user,
            claim_earned_points,
        );
        let claim_signature = backend
            .send_and_retry("Claim rewards", &[instruction], &[])
            .await?;
        println!(
            "Claimed {} collateral in transaction: {}",
            native_amount_to_ui_amount(claim_collateral_amount, collateral_mint_decimals),
            claim_signature
        );

        Ok(())
    }
}
