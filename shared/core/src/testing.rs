use std::str::FromStr;

#[derive(Clone, Copy, PartialEq)]
pub enum IntegrationTestLogMarker {
    StateChange,
    Loss,
    LoadedModel,
    HealthCheck,
    UntrainedBatches,
    SolanaSubscription,
    WitnessElected,
    Error,
    DataProviderFetchSuccess,
    DataProviderFetchError,
    RpcFallback,
}

impl std::fmt::Display for IntegrationTestLogMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::StateChange => "state_change",
                Self::Loss => "loss",
                Self::LoadedModel => "loaded_model",
                Self::HealthCheck => "health_check",
                Self::UntrainedBatches => "untrained_batches",
                Self::SolanaSubscription => "solana_subscription",
                Self::WitnessElected => "witness_elected",
                Self::Error => "error",
                Self::DataProviderFetchSuccess => "data_provider_fetch_success",
                Self::DataProviderFetchError => "data_provider_fetch_error",
                Self::RpcFallback => "rpc_fallback",
            }
        )
    }
}

impl FromStr for IntegrationTestLogMarker {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "state_change" => Self::StateChange,
            "loss" => Self::Loss,
            "loaded_model" => Self::LoadedModel,
            "health_check" => Self::HealthCheck,
            "untrained_batches" => Self::UntrainedBatches,
            "solana_subscription" => Self::SolanaSubscription,
            "witness_elected" => Self::WitnessElected,
            "error" => Self::Error,
            "data_provider_fetch_success" => Self::DataProviderFetchSuccess,
            "data_provider_fetch_error" => Self::DataProviderFetchError,
            "rpc_fallback" => Self::RpcFallback,
            _ => return Err(()),
        })
    }
}
