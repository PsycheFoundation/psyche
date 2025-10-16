#![allow(unexpected_cfgs)]
mod client;
mod clients_state;
mod instance_state;
pub mod logic;
mod program_error;

use anchor_lang::prelude::*;
pub use client::Client;
pub use client::ClientId;
pub use instance_state::CoordinatorInstanceState;
use logic::*;
pub use program_error::ProgramError;
use psyche_coordinator::Committee;
use psyche_coordinator::CommitteeProof;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorProgress;
use psyche_coordinator::SOLANA_MAX_NUM_CLIENTS;
use psyche_coordinator::SOLANA_MAX_STRING_LEN;
use psyche_coordinator::Witness;
use psyche_coordinator::WitnessBloom;
use psyche_coordinator::WitnessMetadata;
use psyche_coordinator::WitnessProof;
use psyche_coordinator::model::{HubRepo, Model};
use psyche_core::MerkleRoot;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

pub use crate::instance_state::RunMetadata;

declare_id!("HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y");

pub const SOLANA_MAX_NUM_PENDING_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;

pub fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..SOLANA_MAX_STRING_LEN.min(str.len())]
}

pub fn find_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[CoordinatorInstance::SEEDS_PREFIX, bytes_from_string(run_id)],
        &crate::ID,
    )
    .0
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializeCoordinatorFromBytes {
    #[error(
        "Coordinator has an incorrect size. Expected {expected}, got {actual}."
    )]
    IncorrectSize { expected: usize, actual: usize },

    #[error(
        "Coordinator has an invalid discriminator. Expected {expected:?}, got {actual:?}."
    )]
    InvalidDiscriminator { expected: Vec<u8>, actual: Vec<u8> },

    #[error("Failed to cast bytes into CoordinatorAccount: {0}")]
    CastError(#[from] bytemuck::PodCastError),
}

fn validate_coordinator_account_bytes(
    bytes: &[u8],
) -> std::result::Result<(), DeserializeCoordinatorFromBytes> {
    if bytes.len() != CoordinatorAccount::space_with_discriminator() {
        return Err(DeserializeCoordinatorFromBytes::IncorrectSize {
            expected: CoordinatorAccount::space_with_discriminator(),
            actual: bytes.len(),
        });
    }
    if &bytes[..CoordinatorAccount::DISCRIMINATOR.len()]
        != CoordinatorAccount::DISCRIMINATOR
    {
        return Err(DeserializeCoordinatorFromBytes::InvalidDiscriminator {
            expected: CoordinatorAccount::DISCRIMINATOR.to_vec(),
            actual: bytes[..CoordinatorAccount::DISCRIMINATOR.len()].to_vec(),
        });
    }
    Ok(())
}

pub fn coordinator_account_from_bytes(
    bytes: &[u8],
) -> std::result::Result<&CoordinatorAccount, DeserializeCoordinatorFromBytes> {
    validate_coordinator_account_bytes(bytes)?;

    Ok(bytemuck::try_from_bytes(
        &bytes[CoordinatorAccount::DISCRIMINATOR.len()
            ..CoordinatorAccount::space_with_discriminator()],
    )?)
}

pub fn coordinator_account_from_bytes_mut(
    bytes: &mut [u8],
) -> std::result::Result<&mut CoordinatorAccount, DeserializeCoordinatorFromBytes>
{
    validate_coordinator_account_bytes(bytes)?;

    Ok(bytemuck::try_from_bytes_mut(
        &mut bytes[CoordinatorAccount::DISCRIMINATOR.len()
            ..CoordinatorAccount::space_with_discriminator()],
    )?)
}

#[account(zero_copy)]
#[repr(C)]
#[derive(Serialize, Deserialize, TS)]
pub struct CoordinatorAccount {
    pub state: CoordinatorInstanceState,
    pub nonce: u64,
}

impl CoordinatorAccount {
    pub fn space_with_discriminator() -> usize {
        CoordinatorAccount::DISCRIMINATOR.len()
            + std::mem::size_of::<CoordinatorAccount>()
    }

    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
        msg!("Nonce: {}", self.nonce);
    }
}

#[derive(Debug, InitSpace)]
#[account]
pub struct CoordinatorInstance {
    pub bump: u8,
    pub main_authority: Pubkey,
    pub join_authority: Pubkey,
    pub coordinator_account: Pubkey,
    #[max_len(SOLANA_MAX_STRING_LEN)]
    pub run_id: String,
}

impl CoordinatorInstance {
    pub const SEEDS_PREFIX: &'static [u8] = b"coordinator";
}

#[program]
pub mod psyche_solana_coordinator {

    use super::*;
    use psyche_core::FixedString;

    pub fn init_coordinator(
        context: Context<InitCoordinatorAccounts>,
        params: InitCoordinatorParams,
    ) -> Result<()> {
        init_coordinator_processor(context, params)
    }

    pub fn free_coordinator(
        context: Context<FreeCoordinatorAccounts>,
        params: FreeCoordinatorParams,
    ) -> Result<()> {
        free_coordinator_processor(context, params)
    }

    pub fn update(
        ctx: Context<OwnerCoordinatorAccounts>,
        metadata: Option<RunMetadata>,
        config: Option<CoordinatorConfig>,
        model: Option<Model>,
        progress: Option<CoordinatorProgress>,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.update(metadata, config, model, progress)
    }

    pub fn update_version_tag(
        ctx: Context<OwnerCoordinatorAccounts>,
        new_tag: String,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.state.client_version =
            FixedString::<32>::try_from(new_tag.as_str()).unwrap();
        msg!("new tag: {}", account.state.client_version);
        Ok(())
    }

    pub fn set_future_epoch_rates(
        ctx: Context<OwnerCoordinatorAccounts>,
        epoch_earning_rate: Option<u64>,
        epoch_slashing_rate: Option<u64>,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account
            .state
            .set_future_epoch_rates(epoch_earning_rate, epoch_slashing_rate)
    }

    pub fn join_run(
        context: Context<JoinRunAccounts>,
        params: JoinRunParams,
    ) -> Result<()> {
        join_run_processor(context, params)
    }

    pub fn set_paused(
        ctx: Context<OwnerCoordinatorAccounts>,
        paused: bool,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.set_paused(paused)
    }

    pub fn tick(ctx: Context<PermissionlessCoordinatorAccounts>) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.tick()
    }

    #[allow(unused_variables)] // for the metadata field. adding a _ prefix results in anchor's IDL not matching the actual types. lol.
    pub fn witness(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        proof: WitnessProof,
        participant_bloom: WitnessBloom,
        broadcast_bloom: WitnessBloom,
        broadcast_merkle: MerkleRoot,
        metadata: WitnessMetadata,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.witness(
            ctx.accounts.user.key,
            Witness {
                proof,
                participant_bloom,
                broadcast_bloom,
                broadcast_merkle,
            },
        )
    }

    #[allow(unused_variables)] // for the metadata field. adding a _ prefix results in anchor's IDL not matching the actual types. lol.
    pub fn warmup_witness(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        proof: WitnessProof,
        participant_bloom: WitnessBloom,
        broadcast_bloom: WitnessBloom,
        broadcast_merkle: MerkleRoot,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.warmup_witness(
            ctx.accounts.user.key,
            Witness {
                proof,
                participant_bloom,
                broadcast_bloom,
                broadcast_merkle,
            },
        )
    }

    pub fn health_check(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        id: ClientId,
        committee: Committee,
        position: u64,
        index: u64,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.health_check(
            ctx.accounts.user.key,
            vec![(
                id,
                CommitteeProof {
                    committee,
                    position,
                    index,
                },
            )],
        )
    }

    pub fn checkpoint(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        repo: HubRepo,
    ) -> Result<()> {
        let mut account = ctx.accounts.coordinator_account.load_mut()?;
        account.increment_nonce();
        account.state.checkpoint(ctx.accounts.user.key, repo)
    }
}

#[derive(Accounts)]
pub struct OwnerCoordinatorAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump,
        constraint = coordinator_instance.main_authority == authority.key()
    )]
    pub coordinator_instance: Box<Account<'info, CoordinatorInstance>>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key()
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}

#[derive(Accounts)]
pub struct PermissionlessCoordinatorAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump
    )]
    pub coordinator_instance: Box<Account<'info, CoordinatorInstance>>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key()
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}
