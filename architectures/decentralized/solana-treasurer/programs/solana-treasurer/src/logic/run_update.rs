use anchor_lang::prelude::*;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorProgress;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::Model;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::CoordinatorInstance;
use psyche_solana_coordinator::RunMetadata;
use psyche_solana_coordinator::cpi::accounts::OwnerCoordinatorAccounts;
use psyche_solana_coordinator::cpi::set_future_epoch_rates;
use psyche_solana_coordinator::cpi::set_paused;
use psyche_solana_coordinator::cpi::update;
use psyche_solana_coordinator::cpi::update_client_version;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: RunUpdateParams)]
pub struct RunUpdateAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        constraint = run.main_authority == authority.key(),
        constraint = run.coordinator_instance == coordinator_instance.key(),
        constraint = run.coordinator_account == coordinator_account.key(),
    )]
    pub run: Box<Account<'info, Run>>,

    #[account()]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(mut)]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunUpdateParams {
    pub metadata: Option<RunMetadata>,
    pub config: Option<CoordinatorConfig>,
    pub model: Option<Model>,
    pub progress: Option<CoordinatorProgress>,
    pub epoch_earning_rate_total_shared: Option<u64>,
    pub epoch_slashing_rate_per_client: Option<u64>,
    pub paused: Option<bool>,
    pub client_version: Option<String>,
    pub data_location: Option<LLMTrainingDataLocation>,
}

pub fn run_update_processor(
    context: Context<RunUpdateAccounts>,
    params: RunUpdateParams,
) -> Result<()> {
    let run = &context.accounts.run;
    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.index.to_le_bytes(), &[run.bump]]];

    if params.metadata.is_some()
        || params.config.is_some()
        || params.model.is_some()
        || params.progress.is_some()
    {
        update(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    coordinator_instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    coordinator_account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            params.metadata,
            params.config,
            params.model,
            params.progress,
        )?;
    }

    if params.epoch_earning_rate_total_shared.is_some()
        || params.epoch_slashing_rate_per_client.is_some()
    {
        set_future_epoch_rates(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    coordinator_instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    coordinator_account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            params.epoch_earning_rate_total_shared,
            params.epoch_slashing_rate_per_client,
        )?;
    }

    if let Some(data_location) = params.data_location {
        psyche_solana_coordinator::cpi::update_data_locations(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    coordinator_instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    coordinator_account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            Some(data_location),
        )?;
    }

    if let Some(paused) = params.paused {
        set_paused(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    coordinator_instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    coordinator_account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            paused,
        )?;
    }

    if let Some(client_version) = params.client_version {
        update_client_version(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    coordinator_instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    coordinator_account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            client_version,
        )?;
    }

    Ok(())
}
