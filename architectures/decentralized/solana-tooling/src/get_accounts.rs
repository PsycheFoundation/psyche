use anchor_lang::AccountDeserialize;
use anyhow::anyhow;
use anyhow::Result;
use psyche_solana_authorizer::state::Authorization;
use psyche_solana_coordinator::coordinator_account_from_bytes;
use psyche_solana_coordinator::CoordinatorInstanceState;
use psyche_solana_treasurer::state::Participant;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn get_authorization(
    endpoint: &mut ToolboxEndpoint,
    authorization: &Pubkey,
) -> Result<Option<Authorization>> {
    endpoint
        .get_account_data(authorization)
        .await?
        .map(|data| {
            Authorization::try_deserialize(&mut data.as_slice()).map_err(
                |err| anyhow!("Unable to decode authorization data: {:?}", err),
            )
        })
        .transpose()
}

pub async fn get_coordinator_account_state(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<Option<CoordinatorInstanceState>> {
    endpoint
        .get_account_data(coordinator_account)
        .await?
        .map(|data| {
            coordinator_account_from_bytes(&data)
                .map_err(|err| {
                    anyhow!(
                        "Unable to decode coordinator_account data: {:?}",
                        err
                    )
                })
                .map(|coordinator_account| coordinator_account.state)
        })
        .transpose()
}

pub async fn get_run(
    endpoint: &mut ToolboxEndpoint,
    run: &Pubkey,
) -> Result<Option<Run>> {
    endpoint
        .get_account_data(run)
        .await?
        .map(|data| {
            Run::try_deserialize(&mut data.as_slice())
                .map_err(|err| anyhow!("Unable to decode run data: {:?}", err))
        })
        .transpose()
}

pub async fn get_participant(
    endpoint: &mut ToolboxEndpoint,
    participant: &Pubkey,
) -> Result<Option<Participant>> {
    endpoint
        .get_account_data(participant)
        .await?
        .map(|data| {
            Participant::try_deserialize(&mut data.as_slice()).map_err(|err| {
                anyhow!("Unable to decode participant data: {:?}", err)
            })
        })
        .transpose()
}
