use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use psyche_coordinator::SOLANA_RUN_ID_MAX_LEN;
use psyche_solana_coordinator::cpi::accounts::InitCoordinatorAccounts;
use psyche_solana_coordinator::cpi::init_coordinator;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

use crate::ProgramError;
use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: RunCreateParams)]
pub struct RunCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Run::space_with_discriminator(),
        seeds = [
            Run::SEEDS_PREFIX,
            params.index.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub run: Box<Account<'info, Run>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = collateral_mint,
        associated_token::authority = run,
    )]
    pub run_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    /// CHECK: This is only used and checked in the CPI to the coordinator program
    #[account(mut)]
    pub coordinator_instance: UncheckedAccount<'info>,

    /// CHECK: This is only used and checked in the CPI to the coordinator program
    #[account(mut)]
    pub coordinator_account: UncheckedAccount<'info>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,

    #[account()]
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunCreateParams {
    pub index: u64,
    pub run_id: String,
    pub client_version: String,
    pub main_authority: Pubkey,
    pub join_authority: Pubkey,
}

pub fn run_create_processor(
    context: Context<RunCreateAccounts>,
    params: RunCreateParams,
) -> Result<()> {
    if params.run_id.len() > SOLANA_RUN_ID_MAX_LEN {
        return err!(ProgramError::RunIdInvalidLength);
    }

    let run = &mut context.accounts.run;
    run.bump = context.bumps.run;
    run.index = params.index;

    run.main_authority = params.main_authority;
    run.join_authority = params.join_authority;

    run.coordinator_instance = context.accounts.coordinator_instance.key();
    run.coordinator_account = context.accounts.coordinator_account.key();

    run.collateral_mint = context.accounts.collateral_mint.key();

    run.total_claimed_collateral_amount = 0;
    run.total_claimed_earned_points = 0;

    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.index.to_le_bytes(), &[run.bump]]];
    init_coordinator(
        CpiContext::new(
            context.accounts.coordinator_program.to_account_info(),
            InitCoordinatorAccounts {
                payer: context.accounts.payer.to_account_info(),
                coordinator_instance: context
                    .accounts
                    .coordinator_instance
                    .to_account_info(),
                coordinator_account: context
                    .accounts
                    .coordinator_account
                    .to_account_info(),
                system_program: context
                    .accounts
                    .system_program
                    .to_account_info(),
            },
        )
        .with_signer(run_signer_seeds),
        InitCoordinatorParams {
            main_authority: context.accounts.run.key(),
            join_authority: params.join_authority,
            run_id: params.run_id.clone(),
            client_version: params.client_version.clone(),
        },
    )?;

    Ok(())
}
