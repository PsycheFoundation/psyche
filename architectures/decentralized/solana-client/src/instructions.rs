use anchor_client::anchor_lang::system_program;
use anchor_client::anchor_lang::InstructionData;
use anchor_client::anchor_lang::ToAccountMetas;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_spl::associated_token;
use anchor_spl::token;
use psyche_coordinator::model::Model;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorProgress;
use psyche_solana_coordinator::RunMetadata;

pub fn coordinator_init_coordinator(
    payer: &Pubkey,
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    join_authority: &Pubkey,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_coordinator::ID,
        accounts: psyche_solana_coordinator::accounts::InitCoordinatorAccounts {
            payer: *payer,
            coordinator_instance,
            coordinator_account: *coordinator_account,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::InitCoordinator {
            params: psyche_solana_coordinator::logic::InitCoordinatorParams {
                main_authority: *main_authority,
                join_authority: *join_authority,
                run_id: run_id.to_string(),
            },
        }
        .data(),
    }
}

pub fn coordinator_update(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    metadata: Option<RunMetadata>,
    config: Option<CoordinatorConfig>,
    model: Option<Model>,
    progress: Option<CoordinatorProgress>,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_coordinator::ID,
        accounts: psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::Update {
            metadata,
            config,
            model,
            progress,
        }
        .data(),
    }
}

pub fn coordinator_set_paused(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    paused: bool,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_coordinator::ID,
        accounts: psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::SetPaused { paused }.data(),
    }
}

pub fn coordinator_set_future_epoch_rates(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    epoch_earning_rate: Option<u64>,
    epoch_slashing_rate: Option<u64>,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_coordinator::ID,
        accounts: psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::SetFutureEpochRates {
            epoch_earning_rate,
            epoch_slashing_rate,
        }
        .data(),
    }
}

pub fn treasurer_run_create(
    payer: &Pubkey,
    run_id: &str,
    treasurer_index: u64,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    join_authority: &Pubkey,
) -> Instruction {
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let run_collateral = associated_token::get_associated_token_address(&run, collateral_mint);
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_treasurer::ID,
        accounts: psyche_solana_treasurer::accounts::RunCreateAccounts {
            payer: *payer,
            run,
            run_collateral,
            collateral_mint: *collateral_mint,
            coordinator_instance,
            coordinator_account: *coordinator_account,
            coordinator_program: psyche_solana_coordinator::ID,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_treasurer::instruction::RunCreate {
            params: psyche_solana_treasurer::logic::RunCreateParams {
                index: treasurer_index,
                main_authority: *main_authority,
                join_authority: *join_authority,
                run_id: run_id.to_string(),
            },
        }
        .data(),
    }
}

pub fn treasurer_run_update(
    run_id: &str,
    treasurer_index: u64,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    params: psyche_solana_treasurer::logic::RunUpdateParams,
) -> Instruction {
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_treasurer::ID,
        accounts: psyche_solana_treasurer::accounts::RunUpdateAccounts {
            authority: *main_authority,
            run,
            coordinator_instance,
            coordinator_account: *coordinator_account,
            coordinator_program: psyche_solana_coordinator::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_treasurer::instruction::RunUpdate { params }.data(),
    }
}
