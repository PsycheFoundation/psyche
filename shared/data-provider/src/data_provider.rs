use crate::{
    DataProviderTcpClient, DummyDataProvider, LocalDataProvider, PreprocessedDataProvider,
    TokenizedData, TokenizedDataProvider, WeightedDataProvider,
    hf_preprocessed::HuggingFacePreprocessedDataProvider, http::HttpDataProvider,
};

use psyche_core::BatchId;

pub enum DataProvider {
    Http(HttpDataProvider),
    Server(DataProviderTcpClient),
    Dummy(DummyDataProvider),
    WeightedHttp(WeightedDataProvider<HttpDataProvider>),
    Local(LocalDataProvider),
    Preprocessed(PreprocessedDataProvider),
    HuggingFacePreprocessed(HuggingFacePreprocessedDataProvider),
}

impl TokenizedDataProvider for DataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> anyhow::Result<Vec<TokenizedData>> {
        match self {
            DataProvider::Http(provider) => provider.get_samples(data_ids).await,
            DataProvider::Server(provider) => provider.get_samples(data_ids).await,
            DataProvider::Dummy(provider) => provider.get_samples(data_ids).await,
            DataProvider::WeightedHttp(provider) => provider.get_samples(data_ids).await,
            DataProvider::Local(provider) => provider.get_samples(data_ids).await,
            DataProvider::Preprocessed(provider) => provider.get_samples(data_ids).await,
            DataProvider::HuggingFacePreprocessed(provider) => provider.get_samples(data_ids).await,
        }
    }
}
