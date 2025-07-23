use crate::{
    DataProviderTcpClient, DummyDataProvider, TokenizedData, TokenizedDataProvider,
    WeightedDataProvider, http::HttpDataProvider,
};

use psyche_core::BatchId;
use psyche_network::AuthenticatableIdentity;

pub enum DataProvider<T: AuthenticatableIdentity> {
    Http(HttpDataProvider),
    Server(DataProviderTcpClient<T>),
    Dummy(DummyDataProvider),
    WeightedHttp(WeightedDataProvider<HttpDataProvider>),
}

impl<T: AuthenticatableIdentity> TokenizedDataProvider for DataProvider<T> {
    async fn get_samples(&mut self, data_ids: BatchId) -> anyhow::Result<Vec<TokenizedData>> {
        match self {
            DataProvider::Http(provider) => provider.get_samples(data_ids).await,
            DataProvider::Server(provider) => provider.get_samples(data_ids).await,
            DataProvider::Dummy(provider) => provider.get_samples(data_ids).await,
            DataProvider::WeightedHttp(provider) => provider.get_samples(data_ids).await,
        }
    }
}
