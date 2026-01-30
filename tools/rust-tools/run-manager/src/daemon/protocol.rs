use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request messages sent from CLI client to daemon
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    Start {
        env_file: PathBuf,
        coordinator_program_id: String,
        local: bool,
        entrypoint: Option<String>,
        entrypoint_args: Vec<String>,
    },
    Stop,
    Restart,
    Status,
    Logs {
        follow: bool,
        lines: Option<usize>,
    },
}

/// Response messages sent from daemon to CLI client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Response {
    Ok,
    Started,
    Stopped,
    Status {
        running: bool,
        container_id: Option<String>,
        image: Option<String>,
        uptime_secs: Option<u64>,
        run_id: Option<String>,
    },
    Logs {
        lines: Vec<String>,
        complete: bool,
    },
    LogLine(String),
    Error {
        message: String,
    },
    AlreadyRunning,
    NotRunning,
}

/// Get the socket path for this user
pub fn get_socket_path() -> PathBuf {
    // Use current working directory for socket file
    PathBuf::from("./run-manager.sock")
}

/// Get the PID file path for this user
pub fn get_pid_path() -> PathBuf {
    // Use current working directory for PID file
    PathBuf::from("./run-manager.pid")
}
