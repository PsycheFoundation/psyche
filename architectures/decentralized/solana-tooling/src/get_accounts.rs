use anchor_lang::AccountDeserialize;
use psyche_solana_authorizer::state::Authorization;
use psyche_solana_coordinator::coordinator_account_from_bytes;
use psyche_solana_coordinator::CoordinatorInstanceState;
use psyche_solana_treasurer::state::Participant;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

pub async fn get_authorization(
    endpoint: &mut ToolboxEndpoint,
    authorization: &Pubkey,
) -> Result<Option<Authorization>, ToolboxEndpointError> {
    endpoint
        .get_account_data(authorization)
        .await?
        .map(|data| {
            Authorization::try_deserialize(&mut data.as_slice()).map_err(|_| {
                ToolboxEndpointError::Custom(
                    "Unable to decode authorization data".to_string(),
                )
            })
        })
        .transpose()
}

pub async fn get_coordinator_account_state(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<Option<CoordinatorInstanceState>, ToolboxEndpointError> {
    endpoint
        .get_account_data(coordinator_account)
        .await?
        .map(|data| {
            coordinator_account_from_bytes(&data)
                .map_err(|_| {
                    ToolboxEndpointError::Custom(
                        "Unable to decode coordinator_account data".to_string(),
                    )
                })
                .map(|coordinator_account| coordinator_account.state)
        })
        .transpose()
}

pub async fn get_run(
    endpoint: &mut ToolboxEndpoint,
    run: &Pubkey,
) -> Result<Option<Run>, ToolboxEndpointError> {
    endpoint
        .get_account_data(run)
        .await?
        .map(|data| {
            Run::try_deserialize(&mut data.as_slice()).map_err(|_| {
                ToolboxEndpointError::Custom(
                    "Unable to decode run data".to_string(),
                )
            })
        })
        .transpose()
}

pub async fn get_participant(
    endpoint: &mut ToolboxEndpoint,
    participant: &Pubkey,
) -> Result<Option<Participant>, ToolboxEndpointError> {
    endpoint
        .get_account_data(participant)
        .await?
        .map(|data| {
            Participant::try_deserialize(&mut data.as_slice()).map_err(|_| {
                ToolboxEndpointError::Custom(
                    "Unable to decode participant data".to_string(),
                )
            })
        })
        .transpose()
}
