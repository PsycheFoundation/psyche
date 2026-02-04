use anyhow::Result;
use async_trait::async_trait;
use psyche_solana_rpc::SolanaBackend;

/// Trait for executable commands that operate on the Solana blockchain
#[async_trait]
pub trait Command {
    async fn execute(self, backend: SolanaBackend) -> Result<()>;
}
