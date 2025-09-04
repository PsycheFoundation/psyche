use std::future::Future;
use std::time::Duration;

use anchor_client::ClientError;
use anchor_client::solana_client::client_error::ClientErrorKind as ErrorKind;
use anchor_client::solana_client::rpc_request::{RpcError, RpcResponseErrorData};
use anchor_client::solana_sdk::transaction::TransactionError;
use backon::{ExponentialBuilder, Retryable};
use tracing::{error, warn};

const DEFAULT_MAX_TIMES: usize = 5;
const DEFAULT_INITIAL_BACKOFF_MS: u64 = 500; // 0.5 seconds
const DEFAULT_MAX_BACKOFF_MS: u64 = 10000; // 10 seconds
const DEFAULT_BACKOFF_FACTOR: f32 = 1.5;

#[derive(Debug)]
pub enum RetryError<E> {
    Retryable(E),
    NonRetryable(E),
    Fatal(E),
}

impl<E: std::fmt::Display> std::fmt::Display for RetryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RetryError::Retryable(e) => write!(f, "{e}"),
            RetryError::NonRetryable(e) => write!(f, "{e}"),
            RetryError::Fatal(e) => write!(f, "{e}"),
        }
    }
}

impl From<ClientError> for RetryError<ClientError> {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::SolanaClientError(e) => {
                match e.kind {
                    // Network/IO errors
                    ErrorKind::Io(ref io_err) => match io_err.kind() {
                        std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::NotConnected
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted => {
                            RetryError::Retryable(ClientError::SolanaClientError(e))
                        }
                        _ => RetryError::NonRetryable(ClientError::SolanaClientError(e)),
                    },
                    // HTTP client errors
                    ErrorKind::Reqwest(ref req_err) => {
                        if req_err.is_timeout() || req_err.is_connect() || req_err.is_status() {
                            RetryError::Retryable(ClientError::SolanaClientError(e))
                        } else {
                            RetryError::NonRetryable(ClientError::SolanaClientError(e))
                        }
                    }
                    ErrorKind::RpcError(ref rpc_err) => match rpc_err {
                        RpcError::RpcResponseError { code, data, .. } => {
                            // Common RPC error codes
                            if let -32602..=-32600 = code {
                                // Invalid requests/params
                                return RetryError::NonRetryable(ClientError::SolanaClientError(e));
                            }

                            if let RpcResponseErrorData::SendTransactionPreflightFailure(sim_res) =
                                data
                            {
                                if sim_res.err.is_some() {
                                    return RetryError::NonRetryable(
                                        ClientError::SolanaClientError(e),
                                    );
                                }
                            };

                            // If we cannot determine via code, check the message
                            let msg = rpc_err.to_string();
                            if msg.contains("InvalidRunState") {
                                error!("InvalidRunState. Fatal Error.");
                                RetryError::Fatal(ClientError::SolanaClientError(e))
                            } else if msg.contains("InvalidWitness") {
                                error!("InvalidWitness. Fatal Error.");
                                RetryError::Fatal(ClientError::SolanaClientError(e))
                            } else if msg.contains("Failed to tick") {
                                warn!("Failed to tick. Retryable Error.");
                                RetryError::Retryable(ClientError::SolanaClientError(e))
                            } else if msg.contains("already been processed") {
                                warn!("Transaction already processed. NonRetryable Error");
                                RetryError::NonRetryable(ClientError::SolanaClientError(e))
                            } else {
                                warn!("Unknown error. Retryable Error.");
                                RetryError::Retryable(ClientError::SolanaClientError(e))
                            }
                        }
                        _ => RetryError::Retryable(ClientError::SolanaClientError(e)),
                    },
                    ErrorKind::TransactionError(ref tx_err) => match tx_err {
                        TransactionError::BlockhashNotFound
                        | TransactionError::ClusterMaintenance
                        | TransactionError::WouldExceedMaxBlockCostLimit
                        | TransactionError::WouldExceedMaxAccountCostLimit
                        | TransactionError::WouldExceedAccountDataBlockLimit
                        | TransactionError::TooManyAccountLocks
                        | TransactionError::AddressLookupTableNotFound
                        | TransactionError::WouldExceedMaxVoteCostLimit
                        | TransactionError::WouldExceedAccountDataTotalLimit
                        | TransactionError::MaxLoadedAccountsDataSizeExceeded
                        | TransactionError::ResanitizationNeeded
                        | TransactionError::ProgramExecutionTemporarilyRestricted { .. }
                        | TransactionError::ProgramCacheHitMaxLimit
                        | TransactionError::AccountBorrowOutstanding
                        | TransactionError::UnsupportedVersion => {
                            RetryError::Retryable(ClientError::SolanaClientError(e))
                        }
                        _ => RetryError::NonRetryable(ClientError::SolanaClientError(e)),
                    },
                    ErrorKind::SigningError(_) => {
                        RetryError::NonRetryable(ClientError::SolanaClientError(e))
                    }
                    ErrorKind::SerdeJson(_) => {
                        RetryError::NonRetryable(ClientError::SolanaClientError(e))
                    }
                    ErrorKind::Middleware(ref middleware_err) => {
                        if middleware_err.to_string().contains("timeout")
                            || middleware_err.to_string().contains("connection")
                        {
                            RetryError::Retryable(ClientError::SolanaClientError(e))
                        } else {
                            RetryError::NonRetryable(ClientError::SolanaClientError(e))
                        }
                    }
                    ErrorKind::Custom(ref msg) => {
                        if msg.contains("permanent") || msg.contains("invalid") {
                            RetryError::NonRetryable(ClientError::SolanaClientError(e))
                        } else {
                            RetryError::Retryable(ClientError::SolanaClientError(e))
                        }
                    }
                }
            }
            ClientError::AccountNotFound => RetryError::NonRetryable(e),
            ClientError::ProgramError(_) => RetryError::NonRetryable(e),
            _ => RetryError::Retryable(e), // By default use retryable and attempt to retry.
        }
    }
}

impl From<RetryError<ClientError>> for RetryError<String> {
    fn from(e: RetryError<ClientError>) -> Self {
        match e {
            RetryError::Retryable(e) => RetryError::retryable_error(&e.to_string()),
            RetryError::NonRetryable(e) => RetryError::non_retryable_error(&e.to_string()),
            RetryError::Fatal(e) => RetryError::fatal_error(&e.to_string()),
        }
    }
}

impl From<anchor_client::solana_client::client_error::ClientError> for RetryError<ClientError> {
    fn from(e: anchor_client::solana_client::client_error::ClientError) -> Self {
        // Here we are considering all of these as retryable but we might want to check later if that's the case.
        RetryError::Retryable(anchor_client::ClientError::SolanaClientError(e))
    }
}

impl RetryError<String> {
    pub fn retryable_error(msg: &str) -> Self {
        RetryError::Retryable(msg.to_string())
    }

    pub fn non_retryable_error(msg: &str) -> Self {
        RetryError::NonRetryable(msg.to_string())
    }

    pub fn fatal_error(msg: &str) -> Self {
        RetryError::Fatal(format!("FATAL {msg}"))
    }
}

#[allow(dead_code)] // TODO (vbrunet) - shall we re-use this for the generic retry mechanism
pub async fn retry_function<FutureFn, Fut, T, E>(
    log_str: &str,
    function: FutureFn,
) -> Result<T, RetryError<E>>
where
    Fut: Future<Output = Result<T, RetryError<E>>>,
    FutureFn: FnMut() -> Fut,
{
    retry_function_with_params(
        log_str,
        function,
        DEFAULT_INITIAL_BACKOFF_MS,
        DEFAULT_BACKOFF_FACTOR,
        DEFAULT_MAX_TIMES,
        DEFAULT_MAX_BACKOFF_MS,
    )
    .await
}

pub async fn retry_function_with_params<FutureFn, Fut, T, E>(
    log_str: &str,
    function: FutureFn,
    min_delay: u64,
    factor: f32,
    max_times: usize,
    max_delay: u64,
) -> Result<T, RetryError<E>>
where
    Fut: Future<Output = Result<T, RetryError<E>>>,
    FutureFn: FnMut() -> Fut,
{
    let backoff = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(min_delay))
        .with_max_times(max_times)
        .with_factor(factor)
        .with_max_delay(Duration::from_secs(max_delay));

    function
        .retry(backoff)
        .sleep(tokio::time::sleep)
        .when(|e| matches!(e, RetryError::Retryable(_)))
        .notify(|_err: &RetryError<E>, dur: Duration| {
            warn!("[RETRY] {} retrying after {:?}", log_str, dur);
        })
        .await
}
