use anchor_client::anchor_lang::InstructionData;
use anchor_client::anchor_lang::ToAccountMetas;
use anchor_client::anchor_lang::system_program;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_spl::associated_token;
use anchor_spl::token;

pub fn coordinator_init_coordinator(
    payer: &Pubkey,
    run_id: &str,
    client_version: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    join_authority: &Pubkey,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::InitCoordinatorAccounts {
            payer: *payer,
            coordinator_instance,
            coordinator_account: *coordinator_account,
            system_program: system_program::ID,
        },
        psyche_solana_coordinator::instruction::InitCoordinator {
            params: psyche_solana_coordinator::logic::InitCoordinatorParams {
                main_authority: *main_authority,
                join_authority: *join_authority,
                run_id: run_id.to_string(),
                client_version: client_version.to_string(),
            },
        },
    )
}

pub fn coordinator_close_run(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::FreeCoordinatorAccounts {
            authority: *main_authority,
            spill: *main_authority,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::FreeCoordinator {
            params: psyche_solana_coordinator::logic::FreeCoordinatorParams {},
        },
    )
}

pub fn coordinator_update(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    metadata: Option<psyche_solana_coordinator::RunMetadata>,
    config: Option<psyche_coordinator::CoordinatorConfig>,
    model: Option<psyche_coordinator::model::Model>,
    progress: Option<psyche_coordinator::CoordinatorProgress>,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::Update {
            metadata,
            config,
            model,
            progress,
        },
    )
}

pub fn coordinator_set_paused(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    paused: bool,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::SetPaused { paused },
    )
}

pub fn coordinator_join_run(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    authorization: &Pubkey,
    client_id: psyche_solana_coordinator::ClientId,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::JoinRunAccounts {
            user: client_id.signer,
            authorization: *authorization,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::JoinRun {
            params: psyche_solana_coordinator::logic::JoinRunParams { client_id },
        },
    )
}

pub fn coordinator_tick(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
            user: *user,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::Tick {},
    )
}

pub fn coordinator_witness(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
    witness: psyche_coordinator::Witness,
    metadata: psyche_coordinator::WitnessMetadata,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
            user: *user,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::Witness {
            proof: witness.proof,
            participant_bloom: witness.participant_bloom,
            broadcast_bloom: witness.broadcast_bloom,
            broadcast_merkle: witness.broadcast_merkle,
            metadata,
        },
    )
}

pub fn coordinator_warmup_witness(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
    witness: psyche_coordinator::Witness,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
            user: *user,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::WarmupWitness {
            proof: witness.proof,
            participant_bloom: witness.participant_bloom,
            broadcast_bloom: witness.broadcast_bloom,
            broadcast_merkle: witness.broadcast_merkle,
        },
    )
}

pub fn coordinator_health_check(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
    client_id: psyche_solana_coordinator::ClientId,
    check: psyche_coordinator::CommitteeProof,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
            user: *user,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::HealthCheck {
            id: client_id,
            committee: check.committee,
            position: check.position,
            index: check.index,
        },
    )
}

pub fn coordinator_checkpoint(
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
    repo: psyche_coordinator::model::HubRepo,
) -> Instruction {
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
            user: *user,
            coordinator_instance: *coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::Checkpoint { repo },
    )
}

pub fn coordinator_update_client_version(
    run_id: &str,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    new_version: &str,
) -> Instruction {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    anchor_instruction(
        psyche_solana_coordinator::ID,
        psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
            authority: *main_authority,
            coordinator_instance,
            coordinator_account: *coordinator_account,
        },
        psyche_solana_coordinator::instruction::UpdateClientVersion {
            new_version: new_version.to_string(),
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn treasurer_run_create(
    payer: &Pubkey,
    run_id: &str,
    client_version: &str,
    treasurer_index: u64,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    main_authority: &Pubkey,
    join_authority: &Pubkey,
) -> Instruction {
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let run_collateral = associated_token::get_associated_token_address(&run, collateral_mint);
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(run_id);
    anchor_instruction(
        psyche_solana_treasurer::ID,
        psyche_solana_treasurer::accounts::RunCreateAccounts {
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
        },
        psyche_solana_treasurer::instruction::RunCreate {
            params: psyche_solana_treasurer::logic::RunCreateParams {
                index: treasurer_index,
                main_authority: *main_authority,
                join_authority: *join_authority,
                run_id: run_id.to_string(),
                client_version: client_version.to_string(),
            },
        },
    )
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
    anchor_instruction(
        psyche_solana_treasurer::ID,
        psyche_solana_treasurer::accounts::RunUpdateAccounts {
            authority: *main_authority,
            run,
            coordinator_instance,
            coordinator_account: *coordinator_account,
            coordinator_program: psyche_solana_coordinator::ID,
        },
        psyche_solana_treasurer::instruction::RunUpdate { params },
    )
}

pub fn treasurer_participant_create(
    payer: &Pubkey,
    treasurer_index: u64,
    user: &Pubkey,
) -> Instruction {
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let participant = psyche_solana_treasurer::find_participant(&run, user);
    anchor_instruction(
        psyche_solana_treasurer::ID,
        psyche_solana_treasurer::accounts::ParticipantCreateAccounts {
            payer: *payer,
            run,
            participant,
            user: *user,
            system_program: system_program::ID,
        },
        psyche_solana_treasurer::instruction::ParticipantCreate {
            params: psyche_solana_treasurer::logic::ParticipantCreateParams {},
        },
    )
}

pub fn treasurer_participant_claim(
    treasurer_index: u64,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    user: &Pubkey,
    claim_earned_points: u64,
) -> Instruction {
    let user_collateral = associated_token::get_associated_token_address(user, collateral_mint);
    let run = psyche_solana_treasurer::find_run(treasurer_index);
    let run_collateral = associated_token::get_associated_token_address(&run, collateral_mint);
    let participant = psyche_solana_treasurer::find_participant(&run, user);
    anchor_instruction(
        psyche_solana_treasurer::ID,
        psyche_solana_treasurer::accounts::ParticipantClaimAccounts {
            user: *user,
            user_collateral,
            run,
            run_collateral,
            participant,
            coordinator_account: *coordinator_account,
            token_program: token::ID,
        },
        psyche_solana_treasurer::instruction::ParticipantClaim {
            params: psyche_solana_treasurer::logic::ParticipantClaimParams {
                claim_earned_points,
            },
        },
    )
}

pub fn authorizer_authorization_create(
    payer: &Pubkey,
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
) -> Instruction {
    let authorization = psyche_solana_authorizer::find_authorization(grantor, grantee, scope);
    anchor_instruction(
        psyche_solana_authorizer::ID,
        psyche_solana_authorizer::accounts::AuthorizationCreateAccounts {
            payer: *payer,
            grantor: *grantor,
            authorization,
            system_program: system_program::ID,
        },
        psyche_solana_authorizer::instruction::AuthorizationCreate {
            params: psyche_solana_authorizer::logic::AuthorizationCreateParams {
                grantee: *grantee,
                scope: scope.to_vec(),
            },
        },
    )
}

pub fn authorizer_authorization_grantor_update(
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
    active: bool,
) -> Instruction {
    let authorization = psyche_solana_authorizer::find_authorization(grantor, grantee, scope);
    anchor_instruction(
        psyche_solana_authorizer::ID,
        psyche_solana_authorizer::accounts::AuthorizationGrantorUpdateAccounts {
            grantor: *grantor,
            authorization,
        },
        psyche_solana_authorizer::instruction::AuthorizationGrantorUpdate {
            params: psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams { active },
        },
    )
}

pub fn authorizer_authorization_grantee_update(
    payer: &Pubkey,
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
    delegates_clear: bool,
    delegates_added: Vec<Pubkey>,
) -> Instruction {
    let authorization = psyche_solana_authorizer::find_authorization(grantor, grantee, scope);
    anchor_instruction(
        psyche_solana_authorizer::ID,
        psyche_solana_authorizer::accounts::AuthorizationGranteeUpdateAccounts {
            payer: *payer,
            grantee: *grantee,
            authorization,
            system_program: system_program::ID,
        },
        psyche_solana_authorizer::instruction::AuthorizationGranteeUpdate {
            params: psyche_solana_authorizer::logic::AuthorizationGranteeUpdateParams {
                delegates_clear,
                delegates_added,
            },
        },
    )
}

fn anchor_instruction<Accounts: ToAccountMetas, Args: InstructionData>(
    program_id: Pubkey,
    accounts: Accounts,
    args: Args,
) -> Instruction {
    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: args.data(),
    }
}
