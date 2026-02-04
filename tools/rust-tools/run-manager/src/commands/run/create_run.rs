use std::sync::Arc;

use anchor_client::solana_sdk::native_token::lamports_to_sol;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::Signer;
use anchor_client::solana_sdk::system_instruction;
use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use clap::Args;
use psyche_coordinator::SOLANA_RUN_ID_MAX_LEN;

use crate::commands::Command;
use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCreateRun {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(short, long, env)]
    pub client_version: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
    #[clap(long, env)]
    pub treasurer_collateral_mint: Option<Pubkey>,
    #[clap(long)]
    pub join_authority: Option<Pubkey>,
}

#[async_trait]
impl Command for CommandCreateRun {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            client_version,
            treasurer_index,
            treasurer_collateral_mint,
            join_authority,
        } = self;

        if run_id.len() > SOLANA_RUN_ID_MAX_LEN {
            bail!(
                "run_id must be 32 bytes or less, got {} bytes",
                run_id.len()
            );
        }

        let payer = backend.get_payer();
        let main_authority = payer;
        let join_authority = join_authority.unwrap_or(payer);

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_account_signer = Arc::new(Keypair::new());
        let coordinator_account = coordinator_account_signer.pubkey();

        let space = psyche_solana_coordinator::CoordinatorAccount::space_with_discriminator();
        let rent = backend
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let instruction_create = system_instruction::create_account(
            &payer,
            &coordinator_account,
            rent,
            space as u64,
            &psyche_solana_coordinator::ID,
        );

        if treasurer_index.is_some() && treasurer_collateral_mint.is_none() {
            bail!(
                "The treasurer_index is set, but treasurer_collateral_mint is not. \
                Please provide a collateral mint address if you want to create a run with a treasurer."
            );
        }

        let instruction_init = if let Some(treasurer_collateral_mint) = treasurer_collateral_mint {
            let treasurer_index =
                SolanaBackend::compute_deterministic_treasurer_index(&run_id, treasurer_index);
            instructions::treasurer_run_create(
                &payer,
                &run_id,
                &client_version,
                treasurer_index,
                &treasurer_collateral_mint,
                &coordinator_account,
                &main_authority,
                &join_authority,
            )
        } else {
            instructions::coordinator_init_coordinator(
                &payer,
                &run_id,
                &client_version,
                &coordinator_account,
                &main_authority,
                &join_authority,
            )
        };

        let signature = backend
            .send_and_retry(
                "Create and init run",
                &[instruction_create, instruction_init],
                &[coordinator_account_signer],
            )
            .await?;

        println!("Created run {run_id} with transaction: {signature}");
        println!("Instance account: {coordinator_instance}");
        println!("Coordinator account: {coordinator_account}");

        let locked_lamports = backend.get_balance(&coordinator_account).await?;
        let locked_sols = lamports_to_sol(locked_lamports);
        println!("Locked for storage: {locked_sols:.9} SOL");

        Ok(())
    }
}
