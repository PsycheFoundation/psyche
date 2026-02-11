use std::time::SystemTime;
use std::{sync::Arc, time::Duration};

use crate::CLIENT_CONTAINER_PREFIX;
use bollard::container::KillContainerOptions;
use bollard::{Docker, container::LogsOptions};
use futures_util::StreamExt;
use psyche_core::BatchId;
use psyche_core::IntegrationTestLogMarker;
use serde_json::Value;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone, Copy)]
pub enum StateFilter {
    Warmup,
    RoundTrain,
    RoundWitness,
}

#[derive(Debug)]
pub enum Response {
    StateChange(String, String, String, String, u64, u64),
    Loss(String, u64, u64, Option<f64>),
    LoadedModel(String),
    HealthCheck(String, u64, u64),
    UntrainedBatches(Vec<u64>),
    SolanaSubscription(String, String),
    WitnessElected(String),
    Error(ObservedErrorKind, String),
}

#[derive(thiserror::Error, Debug)]
pub enum DockerWatcherError {
    #[error("could not get current unix timestamp")]
    UnixTimestampError,

    #[error("could not convert duration {:?} into seconds", duration)]
    IntoSecondsError { duration: Duration },

    #[error("logging error: {inner}")]
    LogsError { inner: bollard::errors::Error },

    #[error("Client {0} has crashed")]
    ClientCrashedError(String),

    #[error("Invalid integration test log marker {0}")]
    IntegrationTestLogMarker(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedErrorKind {
    InvalidRunState,
    InvalidWitness,
    Timeout,
    Unknown,
}

impl From<String> for ObservedErrorKind {
    fn from(value: String) -> Self {
        if value.contains("InvalidRunState") {
            return ObservedErrorKind::InvalidRunState;
        }
        if value.contains("InvalidWitness") {
            return ObservedErrorKind::InvalidWitness;
        }
        if value.contains("TIMEOUT") {
            return ObservedErrorKind::Timeout;
        }
        ObservedErrorKind::Unknown
    }
}

pub struct DockerWatcher {
    client: Arc<Docker>,
    log_tx: mpsc::Sender<Response>,
    pub log_rx: mpsc::Receiver<Response>,
}

impl DockerWatcher {
    pub fn new(client: Arc<Docker>) -> Self {
        let (log_tx, log_rx) = mpsc::channel(100);

        Self {
            client,
            log_tx,
            log_rx,
        }
    }

    pub fn monitor_container(
        &self,
        name: &str,
        filters: Vec<IntegrationTestLogMarker>,
    ) -> Result<JoinHandle<Result<(), DockerWatcherError>>, DockerWatcherError> {
        let Ok(duration) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
            return Err(DockerWatcherError::UnixTimestampError);
        };
        let Ok(current_unix_timestamp): Result<i64, _> = duration.as_secs().try_into() else {
            return Err(DockerWatcherError::IntoSecondsError { duration });
        };

        let log_options = Some(LogsOptions::<String> {
            stderr: true,
            stdout: true,
            follow: true,
            since: current_unix_timestamp - 10, // -10 to ensure we read all the docker logs
            ..Default::default()
        });

        let name = name.to_string();
        let client = self.client.clone();
        let log_sender = self.log_tx.clone();
        let monitor_handle = tokio::spawn(async move {
            let mut logs = client.logs(&name, log_options);
            while let Some(log) = logs.next().await {
                let log = match log {
                    Ok(log) => log,
                    Err(e) => return Err(DockerWatcherError::LogsError { inner: e }),
                };
                let Ok(parsed_log): Result<Value, _> =
                    serde_json::from_slice(&log.clone().into_bytes())
                else {
                    continue;
                };

                let Some(log_marker_str) = parsed_log
                    .get("integration_test_log_marker")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        if let Some("ERROR") = parsed_log.get("level").and_then(|l| l.as_str()) {
                            Some("error")
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };

                let log_marker: IntegrationTestLogMarker = log_marker_str
                    .parse::<IntegrationTestLogMarker>()
                    .map_err(|_| {
                        DockerWatcherError::IntegrationTestLogMarker(log_marker_str.to_string())
                    })?;

                let current_filter = filters.iter().find(|f| **f == log_marker);
                let Some(filter) = current_filter else {
                    continue;
                };

                // unwrapping is ok here, if the log has the marker, it should have all those props.
                match filter {
                    IntegrationTestLogMarker::StateChange => {
                        let old_state = parsed_log
                            .get("old_state")
                            .and_then(|v| v.as_str())
                            .unwrap();

                        let new_state = parsed_log
                            .get("new_state")
                            .and_then(|v| v.as_str())
                            .unwrap();

                        if old_state != new_state {
                            let client_id = parsed_log
                                .get("client_id")
                                .and_then(|v| v.as_str())
                                .unwrap();

                            let timestamp = parsed_log
                                .get("timestamp")
                                .and_then(|v| v.as_str())
                                .unwrap();
                            let epoch = parsed_log.get("epoch").and_then(|v| v.as_u64()).unwrap();
                            let step = parsed_log.get("step").and_then(|v| v.as_u64()).unwrap();

                            let response = Response::StateChange(
                                timestamp.to_string(),
                                client_id.to_string(),
                                old_state.to_string(),
                                new_state.to_string(),
                                epoch,
                                step,
                            );

                            if log_sender.send(response).await.is_err() {
                                println!("Probably the test ended so we drop the log sender");
                            }
                        }
                    }
                    IntegrationTestLogMarker::Loss => {
                        let loss = parsed_log.get("loss").and_then(|v| v.as_f64());
                        let client_id = parsed_log
                            .get("client_id")
                            .and_then(|v| v.as_str())
                            .unwrap()
                            .to_string();
                        let epoch = parsed_log.get("epoch").and_then(|v| v.as_u64()).unwrap();
                        let step = parsed_log.get("step").and_then(|v| v.as_u64()).unwrap();
                        let response = Response::Loss(client_id, epoch, step, loss);
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::HealthCheck => {
                        let client_id = parsed_log
                            .get("client_id")
                            .and_then(|v| v.as_str())
                            .unwrap()
                            .to_string();
                        let index = parsed_log.get("index").and_then(|v| v.as_u64()).unwrap();
                        let current_step = parsed_log
                            .get("current_step")
                            .and_then(|v| v.as_u64())
                            .unwrap();
                        let response = Response::HealthCheck(client_id, index, current_step);
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::LoadedModel => {
                        let checkpoint = parsed_log.get("checkpoint").unwrap();
                        let checkpoint = serde_json::from_value(checkpoint.clone()).unwrap();
                        let response = Response::LoadedModel(checkpoint);
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::UntrainedBatches => {
                        if parsed_log.get("target")
                            != Some(&Value::String("untrained_batch".to_string()))
                        {
                            continue;
                        }

                        // extract batch Ids
                        let Some(message) = parsed_log.get("batch_id").and_then(|v| v.as_str())
                        else {
                            println!("Invalid batch_id: {parsed_log:?}");
                            let response = Response::UntrainedBatches(vec![0, 0]);
                            if log_sender.send(response).await.is_err() {
                                println!("Probably the test ended so we drop the log sender");
                            }
                            continue;
                        };
                        let Ok(batch_id_range) = BatchId::from_str(message) else {
                            println!("Invalid batch_id range: {message}");
                            let response = Response::UntrainedBatches(vec![0, 0]);
                            if log_sender.send(response).await.is_err() {
                                println!("Probably the test ended so we drop the log sender");
                            }
                            continue;
                        };
                        let batch_ids = batch_id_range.iter().collect();

                        let response = Response::UntrainedBatches(batch_ids);
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::SolanaSubscription => {
                        let url = parsed_log.get("url").unwrap();

                        let mut response =
                            Response::SolanaSubscription("".to_string(), "".to_string());
                        if parsed_log.get("level").unwrap() == "WARN" {
                            response = Response::SolanaSubscription(
                                url.to_string(),
                                "Subscription Down".to_string(),
                            );
                        }

                        if parsed_log.get("level").unwrap() == "INFO" {
                            response = Response::SolanaSubscription(
                                url.to_string(),
                                "Subscription Up".to_string(),
                            );
                        }
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::WitnessElected => {
                        let is_witness = parsed_log
                            .get("witness")
                            .and_then(|v| v.as_str())
                            .unwrap()
                            .to_string();
                        if is_witness != true.to_string() {
                            continue;
                        }
                        let response = Response::WitnessElected(name.clone());
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                    IntegrationTestLogMarker::Error => {
                        let Some(message) = parsed_log.get("message") else {
                            continue;
                        };

                        let response = Response::Error(
                            ObservedErrorKind::from(message.to_string()),
                            message.to_string(),
                        );

                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                }
            }
            Ok(())
        });

        Ok(monitor_handle)
    }

    pub async fn kill_container(&self, name: &str) -> Result<(), DockerWatcherError> {
        self.client
            .kill_container(name, Some(KillContainerOptions { signal: "SIGKILL" }))
            .await
            .map_err(|err| DockerWatcherError::LogsError { inner: err })
    }

    pub async fn monitor_clients_health(&self, num_clients: u8) -> Result<(), DockerWatcherError> {
        for i in 1..=num_clients {
            let container_name = format!("{CLIENT_CONTAINER_PREFIX}-{i}");
            self.monitor_client_health_by_id(container_name.as_str())
                .await?;
        }
        Ok(())
    }

    pub async fn monitor_client_health_by_id(
        &self,
        container_name: &str,
    ) -> Result<(), DockerWatcherError> {
        let container = self
            .client
            .inspect_container(container_name, None)
            .await
            .unwrap();
        let state = container.state.unwrap();
        match state.status {
            Some(bollard::secret::ContainerStateStatusEnum::DEAD)
            | Some(bollard::secret::ContainerStateStatusEnum::EXITED) => {
                let logs = self.fetch_container_logs(container_name, 150).await;
                eprintln!(
                    "\n========== Last 150 lines from {} ==========",
                    container_name
                );
                eprintln!("{}", logs);
                eprintln!("========== End of logs ==========\n");

                Err(DockerWatcherError::ClientCrashedError(
                    container_name.to_string(),
                ))
            }
            _ => Ok(()),
        }
    }

    /// Fetch the last N lines of logs from a container
    async fn fetch_container_logs(&self, container_name: &str, tail: usize) -> String {
        let log_options = Some(LogsOptions::<String> {
            stderr: true,
            stdout: true,
            follow: false,
            tail: tail.to_string(),
            ..Default::default()
        });

        let mut logs = self.client.logs(container_name, log_options);
        let mut log_lines = Vec::new();

        while let Some(log) = logs.next().await {
            if let Ok(log) = log {
                log_lines.push(log.to_string());
            }
        }

        log_lines.join("\n")
    }
}
