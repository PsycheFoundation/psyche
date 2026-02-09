use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anyhow::Result;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorProgress;
use psyche_coordinator::model::Model;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::RunMetadata;
use psyche_solana_coordinator::accounts::FreeCoordinatorAccounts;
use psyche_solana_coordinator::accounts::InitCoordinatorAccounts;
use psyche_solana_coordinator::accounts::JoinRunAccounts;
use psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts;
use psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts;
use psyche_solana_coordinator::find_coordinator_instance;
use psyche_solana_coordinator::instruction::FreeCoordinator;
use psyche_solana_coordinator::instruction::InitCoordinator;
use psyche_solana_coordinator::instruction::JoinRun;
use psyche_solana_coordinator::instruction::SetFutureEpochRates;
use psyche_solana_coordinator::instruction::SetPaused;
use psyche_solana_coordinator::instruction::Tick;
use psyche_solana_coordinator::instruction::Update;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::logic::FreeCoordinatorParams;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_coordinator::logic::JoinRunParams;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_coordinator_init(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    params: InitCoordinatorParams,
) -> Result<Pubkey> {
    let coordinator_instance = find_coordinator_instance(&params.run_id);
    let accounts = InitCoordinatorAccounts {
        payer: payer.pubkey(),
        coordinator_instance,
        coordinator_account: *coordinator_account,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: InitCoordinator { params }.data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint.process_instruction(payer, instruction).await?;
    Ok(coordinator_instance)
}

pub async fn process_coordinator_free(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    spill: &Pubkey,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
) -> Result<()> {
    let accounts = FreeCoordinatorAccounts {
        authority: authority.pubkey(),
        spill: *spill,
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: FreeCoordinator {
            params: FreeCoordinatorParams {},
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[authority])
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn process_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    metadata: Option<RunMetadata>,
    config: Option<CoordinatorConfig>,
    model: Option<Model>,
    progress: Option<CoordinatorProgress>,
) -> Result<()> {
    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: Update {
            metadata,
            config,
            model,
            progress,
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[authority])
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn process_coordinator_join_run(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    authorization: &Pubkey,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    client_id: ClientId,
    claimer: &Pubkey,
) -> Result<()> {
    let accounts = JoinRunAccounts {
        user: user.pubkey(),
        authorization: *authorization,
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: JoinRun {
            params: JoinRunParams {
                client_id,
                claimer: *claimer,
            },
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}

pub async fn process_coordinator_set_paused(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    paused: bool,
) -> Result<()> {
    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetPaused { paused }.data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[authority])
        .await?;
    Ok(())
}

pub async fn process_coordinator_set_future_epoch_rates(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    epoch_earning_rate_total_shared: Option<u64>,
    epoch_slashing_rate_per_client: Option<u64>,
) -> Result<()> {
    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetFutureEpochRates {
            epoch_earning_rate_total_shared,
            epoch_slashing_rate_per_client,
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[authority])
        .await?;
    Ok(())
}

pub async fn process_coordinator_tick(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
) -> Result<()> {
    let accounts = PermissionlessCoordinatorAccounts {
        user: user.pubkey(),
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: Tick {}.data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}

pub async fn process_coordinator_witness(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    coordinator_instance: &Pubkey,
    coordinator_account: &Pubkey,
    witness: &Witness,
) -> Result<()> {
    let accounts = PermissionlessCoordinatorAccounts {
        user: user.pubkey(),
        coordinator_instance: *coordinator_instance,
        coordinator_account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: witness.data(),
        program_id: psyche_solana_coordinator::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[user])
        .await?;
    Ok(())
}
