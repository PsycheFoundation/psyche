use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use psyche_solana_coordinator::cpi::accounts::InitCoordinatorAccounts;
use psyche_solana_coordinator::cpi::init_coordinator;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

use crate::run_identity_from_string;
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
            run_identity_from_string(&params.run_id).as_ref()
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

    #[account(mut)]
    pub coordinator_instance: UncheckedAccount<'info>,

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
    pub run_id: String,
    pub main_authority: Pubkey,
    pub join_authority: Pubkey,
    pub collateral_amount_per_earned_point: u64,
}

pub fn run_create_processor(
    context: Context<RunCreateAccounts>,
    params: RunCreateParams,
) -> Result<()> {
    let run_identity = run_identity_from_string(&params.run_id);

    let run = &mut context.accounts.run;
    run.bump = context.bumps.run;
    run.identity = run_identity;

    run.main_authority = params.main_authority;
    run.join_authority = params.join_authority;

    run.coordinator_instance = context.accounts.coordinator_instance.key();
    run.coordinator_account = context.accounts.coordinator_account.key();

    run.collateral_mint = context.accounts.collateral_mint.key();
    run.collateral_amount_per_earned_point =
        params.collateral_amount_per_earned_point;

    run.total_funded_collateral_amount = 0;
    run.total_claimed_collateral_amount = 0;
    run.total_claimed_earned_points = 0;

    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.identity.to_bytes(), &[run.bump]]];
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
        },
    )?;

    Ok(())
}
