use anchor_client::solana_sdk::instruction::Instruction;
use anchor_spl::associated_token;

fn instruction_authorizer_authorization_create(
    payer: &Pubkey,
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
) -> Instruction {
    let authorization = psyche_solana_authorizer::find_authorization(
        grantor,
        grantee,
        psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
    );
    Instruction {
        program_id: psyche_solana_authorizer::ID,
        accounts: psyche_solana_authorizer::accounts::AuthorizationCreateAccounts {
            payer,
            grantor,
            authorization,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_authorizer::instruction::AuthorizationCreate {
            params: psyche_solana_authorizer::logic::AuthorizationCreateParams {
                grantee: *grantee,
                scope: scope.to_vec(),
            },
        },
    }
}

fn instruction_authorizer_authorization_grantor_update(
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
    active: bool,
) -> Instruction {
    let authorization = psyche_solana_authorizer::find_authorization(
        grantor,
        grantee,
        psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
    );
    Instruction {
        program_id: psyche_solana_authorizer::ID,
        accounts: psyche_solana_authorizer::accounts::AuthorizationGrantorUpdateAccounts {
            grantor,
            authorization,
        }
        .to_account_metas(None),
        data: psyche_solana_authorizer::instruction::AuthorizationGrantorUpdate {
            params: psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams { active },
        },
    }
}

fn instruction_coordinator_init_coordinator(
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
            payer,
            coordinator_instance,
            coordinator_account,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::InitCoordinator {
            params: psyche_solana_coordinator::logic::InitCoordinatorParams {
                main_authority,
                join_authority,
                run_id: run_id.to_string(),
            },
        }
        .data(),
    }
}

fn instruction_coordinator_update(
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
            authority: main_authority,
            coordinator_instance,
            coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::CoordinatorUpdate {
            metadata,
            config,
            model,
            progress,
        }
        .data(),
    }
}

fn instruction_coordinator_set_paused(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    paused: bool,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_coordinator::ID,
        accounts: psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: main_authority,
            coordinator_instance,
            coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::CoordinatorSetPaused { paused }.data(),
    }
}

fn instruction_coordinator_set_future_epoch_rates(
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
            authority: main_authority,
            coordinator_instance,
            coordinator_account,
        }
        .to_account_metas(None),
        data: psyche_solana_coordinator::instruction::SetFutureEpochRates {
            epoch_earning_rate,
            epoch_slashing_rate,
        }
        .data(),
    }
}

fn instruction_treasurer_run_create(
    payer: &Pubkey,
    run_id: &str,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    join_authority: &Pubkey,
) -> Instruction {
    let treasurer_index = treasurer_index_deterministic_pick(run_id);
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let run_collateral = associated_token::get_associated_token_address(&run, collateral_mint);
    Instruction {
        program_id: psyche_solana_treasurer::ID,
        accounts: psyche_solana_treasurer::accounts::RunCreateAccounts {
            payer,
            run,
            run_collateral,
            collateral_mint,
            coordinator_instance,
            coordinator_account,
            coordinator_program: psyche_solana_coordinator::ID,
            associated_token_program: associated_token_program::ID,
            token_program: token_program::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_treasurer::instruction::RunCreate {
            params: psyche_solana_treasurer::logic::RunCreateParams {
                index: treasurer_index,
                main_authority,
                join_authority,
                run_id: run_id.to_string(),
            },
        }
        .data(),
    }
}

fn instruction_treasurer_run_update(
    run_id: &str,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    params: psyche_solana_treasurer::logic::RunUpdateParams,
) -> Instruction {
    let treasurer_index = treasurer_index_deterministic_pick(run_id);
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    Instruction {
        program_id: psyche_solana_treasurer::ID,
        accounts: psyche_solana_treasurer::accounts::RunUpdateAccounts {
            authority: main_authority,
            run,
            coordinator_instance,
            coordinator_account,
            coordinator_program: psyche_solana_coordinator::ID,
        }
        .to_account_metas(None),
        data: psyche_solana_treasurer::instruction::RunUpdate { params }.data(),
    }
}

fn treasurer_index_deterministic_pick(run_id: &str) -> u64 {
    let hashed = hash(run_id.as_bytes()).to_bytes();
    u64::from_le_bytes(hashed[0..8].try_into().unwrap())
}
