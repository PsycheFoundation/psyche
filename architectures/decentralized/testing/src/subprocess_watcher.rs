use std::collections::HashMap;
use std::sync::Arc;

use psyche_core::IntegrationTestLogMarker;
use serde_json::Value;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use psyche_core::BatchId;

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
    RpcFallback(String, String),
}

#[derive(thiserror::Error, Debug)]
pub enum WatcherError {
    #[error("logging error: {inner}")]
    IoError { inner: std::io::Error },

    #[error("Process {0} has crashed")]
    ProcessCrashedError(String),

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

/// Tracks child processes and their PIDs for signal-based operations.
pub struct ProcessRegistry {
    processes: HashMap<String, u32>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, pid: u32) {
        self.processes.insert(name, pid);
    }

    pub fn unregister(&mut self, name: &str) {
        self.processes.remove(name);
    }

    pub fn get_pid(&self, name: &str) -> Option<u32> {
        self.processes.get(name).copied()
    }

    pub fn running_names(&self) -> Vec<String> {
        self.processes.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.processes.len()
    }
}

pub struct SubprocessWatcher {
    log_tx: mpsc::Sender<Response>,
    pub log_rx: mpsc::Receiver<Response>,
    pub registry: Arc<Mutex<ProcessRegistry>>,
}

impl SubprocessWatcher {
    pub fn new() -> Self {
        let (log_tx, log_rx) = mpsc::channel(100);
        Self {
            log_tx,
            log_rx,
            registry: Arc::new(Mutex::new(ProcessRegistry::new())),
        }
    }

    /// Start monitoring a child process's stdout for integration test log markers.
    /// This is the subprocess equivalent of DockerWatcher::monitor_container.
    pub fn monitor_process(
        &self,
        name: &str,
        stdout: ChildStdout,
        filters: Vec<IntegrationTestLogMarker>,
    ) -> JoinHandle<Result<(), WatcherError>> {
        let name = name.to_string();
        let log_sender = self.log_tx.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Some(line) = lines
                .next_line()
                .await
                .map_err(|e| WatcherError::IoError { inner: e })?
            {
                let Ok(parsed_log): Result<Value, _> = serde_json::from_str(&line) else {
                    // Not JSON — skip (could be plain text output from solana tools etc)
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
                        WatcherError::IntegrationTestLogMarker(log_marker_str.to_string())
                    })?;

                let current_filter = filters.iter().find(|f| **f == log_marker);
                let Some(filter) = current_filter else {
                    continue;
                };

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
                    IntegrationTestLogMarker::RpcFallback => {
                        let failed_rpc_index = parsed_log
                            .get("failed_rpc_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            .to_string();
                        let error = parsed_log
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let response = Response::RpcFallback(failed_rpc_index, error);
                        if log_sender.send(response).await.is_err() {
                            println!("Probably the test ended so we drop the log sender");
                        }
                    }
                }
            }
            Ok(())
        })
    }

    /// Send a signal to a tracked process by name.
    pub async fn kill_process(&self, name: &str) -> Result<(), WatcherError> {
        let registry = self.registry.lock().await;
        let pid = registry.get_pid(name).ok_or_else(|| {
            WatcherError::ProcessCrashedError(format!("process {name} not found in registry"))
        })?;

        // SIGKILL the process
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        Ok(())
    }

    /// Check if a process is still alive by checking if the PID exists.
    pub async fn check_process_health(&self, name: &str) -> Result<(), WatcherError> {
        let registry = self.registry.lock().await;
        let pid = registry.get_pid(name).ok_or_else(|| {
            WatcherError::ProcessCrashedError(format!("process {name} not found in registry"))
        })?;

        // signal 0 checks if process exists without sending a signal
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result != 0 {
            return Err(WatcherError::ProcessCrashedError(name.to_string()));
        }
        Ok(())
    }

    /// Check health of all client processes matching the prefix.
    pub async fn monitor_clients_health(&self, num_clients: u8) -> Result<(), WatcherError> {
        for i in 1..=num_clients {
            let name = format!("client-{i}");
            self.check_process_health(&name).await?;
        }
        Ok(())
    }

    /// Check health of a specific client by its full name.
    pub async fn monitor_client_health_by_id(&self, name: &str) -> Result<(), WatcherError> {
        self.check_process_health(name).await
    }
}
