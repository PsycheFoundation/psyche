use anchor_lang::prelude::*;
use psyche_coordinator::CoordinatorError;

#[error_code]
pub enum ProgramError {
    #[msg("Cannot update config of finished run")]
    UpdateConfigFinished,

    #[msg("Cannot update config when not halted")]
    UpdateConfigNotHalted,

    #[msg("Coordinator account incorrect size")]
    CoordinatorAccountIncorrectSize,

    #[msg("Coordinator account invalid discriminator")]
    CoordinatorAccountInvalidDiscriminator,

    #[msg("Client id mismatch")]
    ClientIdMismatch,

    #[msg("Clients list full")]
    ClientsFull,

    #[msg("Config sanity check failed")]
    ConfigSanityCheckFailed,

    #[msg("Model sanity check failed")]
    ModelSanityCheckFailed,

    #[msg("Signer not a client")]
    SignerNotAClient,

    #[msg("Signer mismatch")]
    SignerMismatch,

    #[msg("Cannot close coordinator account when not halted")]
    CloseCoordinatorNotHalted,

    #[msg("Coordinator error: No active round")]
    CoordinatorErrorNoActiveRound,

    #[msg("Coordinator error: Invalid witness")]
    CoordinatorErrorInvalidWitness,

    #[msg("Coordinator error: Invalid run state")]
    CoordinatorErrorInvalidRunState,

    #[msg("Coordinator error: Duplicate witness")]
    CoordinatorErrorDuplicateWitness,

    #[msg("Coordinator error: Invalid health check")]
    CoordinatorErrorInvalidHealthCheck,

    #[msg("Coordinator error: Halted")]
    CoordinatorErrorHalted,

    #[msg("Coordinator error: Invalid checkpoint")]
    CoordinatorErrorInvalidCheckpoint,

    #[msg("Coordinator error: Witnesses full")]
    CoordinatorErrorWitnessesFull,

    #[msg("Coordinator error: Cannot resume")]
    CoordinatorErrorCannotResume,

    #[msg("Coordinator error: Invalid withdraw")]
    CoordinatorErrorInvalidWithdraw,

    #[msg("Coordinator error: Invalid committee selection")]
    CoordinatorErrorInvalidCommitteeSelection,
}

impl From<CoordinatorError> for ProgramError {
    fn from(value: CoordinatorError) -> Self {
        match value {
            CoordinatorError::NoActiveRound => {
                ProgramError::CoordinatorErrorNoActiveRound
            },
            CoordinatorError::InvalidWitness => {
                ProgramError::CoordinatorErrorInvalidWitness
            },
            CoordinatorError::InvalidRunState => {
                ProgramError::CoordinatorErrorInvalidRunState
            },
            CoordinatorError::DuplicateWitness => {
                ProgramError::CoordinatorErrorDuplicateWitness
            },
            CoordinatorError::InvalidHealthCheck => {
                ProgramError::CoordinatorErrorInvalidHealthCheck
            },
            CoordinatorError::Halted => {
                ProgramError::CoordinatorErrorNoActiveRound
            },
            CoordinatorError::InvalidCheckpoint => {
                ProgramError::CoordinatorErrorInvalidCheckpoint
            },
            CoordinatorError::WitnessesFull => {
                ProgramError::CoordinatorErrorWitnessesFull
            },
            CoordinatorError::CannotResume => {
                ProgramError::CoordinatorErrorCannotResume
            },
            CoordinatorError::InvalidWithdraw => {
                ProgramError::CoordinatorErrorInvalidWithdraw
            },
            CoordinatorError::InvalidCommitteeSelection => {
                ProgramError::CoordinatorErrorInvalidCommitteeSelection
            },
        }
    }
}
