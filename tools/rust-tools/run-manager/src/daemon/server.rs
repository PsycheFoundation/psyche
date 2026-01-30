use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{error, info, warn};

use super::log_buffer::LogBuffer;
use super::protocol::{Request, Response, get_pid_path, get_socket_path};
use super::state::{ContainerInfo, DaemonState, RunConfig};
use crate::docker::manager::{Entrypoint, RunManager};
use crate::get_env_var;
use crate::load_and_apply_env_file;

const VERSION_MISMATCH_EXIT_CODE: i32 = 10;
const RETRY_DELAY_SECS: u64 = 5;

/// Start the daemon in the background (double-fork to detach)
pub fn start_daemon(
    env_file: PathBuf,
    coordinator_program_id: String,
    local: bool,
    entrypoint: Option<String>,
    entrypoint_args: Vec<String>,
) -> Result<()> {
    let socket_path = get_socket_path();

    // Check if daemon is already running
    if socket_path.exists() {
        // Try to connect to see if it's alive
        if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
            println!("Daemon is already running");
            return Ok(());
        }
        // Stale socket, remove it
        std::fs::remove_file(&socket_path)?;
    }

    // Get current executable path
    let exe = std::env::current_exe().context("Failed to get current executable")?;

    // Build args for the daemon process
    let mut args = vec![
        "__daemon-server".to_string(),
        "--env-file".to_string(),
        env_file.to_string_lossy().to_string(),
        "--coordinator-program-id".to_string(),
        coordinator_program_id,
    ];
    if local {
        args.push("--local".to_string());
    }
    if let Some(ep) = entrypoint {
        args.push("--entrypoint".to_string());
        args.push(ep);
    }
    if !entrypoint_args.is_empty() {
        args.push("--".to_string());
        args.extend(entrypoint_args);
    }

    // Double-fork to daemonize
    match unsafe { libc::fork() } {
        -1 => return Err(anyhow!("First fork failed")),
        0 => {
            // Child process - create new session
            if unsafe { libc::setsid() } == -1 {
                std::process::exit(1);
            }

            // Second fork
            match unsafe { libc::fork() } {
                -1 => std::process::exit(1),
                0 => {
                    // Grandchild - this becomes the daemon
                    // Redirect stdin/stdout/stderr to /dev/null
                    let dev_null =
                        std::fs::File::open("/dev/null").expect("Failed to open /dev/null");
                    let dev_null_fd = std::os::unix::io::AsRawFd::as_raw_fd(&dev_null);
                    unsafe {
                        libc::dup2(dev_null_fd, 0); // stdin
                        libc::dup2(dev_null_fd, 1); // stdout
                        libc::dup2(dev_null_fd, 2); // stderr
                    }

                    // Change to root directory to avoid holding any mounts
                    let _ = std::env::set_current_dir("/");

                    // Exec the daemon server
                    let err = exec::execvp(&exe, &args);
                    eprintln!("exec failed: {}", err);
                    std::process::exit(1);
                }
                _ => {
                    // First child exits immediately
                    std::process::exit(0);
                }
            }
        }
        child_pid => {
            // Parent waits for first child to exit
            let mut status: i32 = 0;
            unsafe {
                libc::waitpid(child_pid, &mut status, 0);
            }
        }
    }

    // Give the daemon a moment to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify it started
    if socket_path.exists() {
        println!("Daemon started successfully");
        Ok(())
    } else {
        Err(anyhow!("Daemon failed to start"))
    }
}

/// Run the daemon server (called by __daemon-server subcommand)
pub async fn run_server(
    env_file: PathBuf,
    coordinator_program_id: String,
    local: bool,
    entrypoint: Option<String>,
    entrypoint_args: Vec<String>,
) -> Result<()> {
    let socket_path = get_socket_path();
    let pid_path = get_pid_path();

    // Clean up any stale socket
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    // Write PID file
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path).context("Failed to bind Unix socket")?;
    info!("Daemon listening on {:?}", socket_path);

    // Shared state
    let state = Arc::new(DaemonState::new());
    let log_buffer = LogBuffer::with_default_capacity();

    // Shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Start the run manager task
    let run_state = state.clone();
    let run_logs = log_buffer.clone();
    let run_shutdown = shutdown_rx.clone();
    let config = RunConfig {
        env_file,
        coordinator_program_id,
        local,
        entrypoint,
        entrypoint_args,
    };
    let run_handle =
        tokio::spawn(
            async move { run_manager_loop(config, run_state, run_logs, run_shutdown).await },
        );

    // Accept connections
    loop {
        let mut shutdown_recv = shutdown_rx.clone();
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let state = state.clone();
                        let log_buffer = log_buffer.clone();
                        let shutdown_tx = shutdown_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, state, log_buffer, shutdown_tx).await {
                                error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
            _ = shutdown_recv.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Shutdown signal received, exiting");
                    break;
                }
            }
        }
    }

    // Wait for run manager to finish
    let _ = run_handle.await;

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&pid_path);

    Ok(())
}

async fn handle_connection(
    stream: UnixStream,
    state: Arc<DaemonState>,
    log_buffer: Arc<LogBuffer>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = TokioBufReader::new(reader);
    let mut line = String::new();

    reader.read_line(&mut line).await?;
    let request: Request = serde_json::from_str(&line)?;

    let response = match request {
        Request::Status => {
            let (running, container_id, image, uptime_secs, run_id) = state.get_status().await;
            Response::Status {
                running,
                container_id,
                image,
                uptime_secs,
                run_id,
            }
        }
        Request::Stop => {
            info!("Stop requested");
            state.request_shutdown().await;

            // Stop the container if running
            if let Some(container_id) = state.get_container_id().await {
                let _ = stop_container(&container_id);
            }

            // Signal main loop to exit
            let _ = shutdown_tx.send(true);
            Response::Stopped
        }
        Request::Restart => {
            if state.get_restart_config().await.is_some() {
                info!("Restart requested");
                // Stop current container
                if let Some(container_id) = state.get_container_id().await {
                    let _ = stop_container(&container_id);
                }
                state.set_stopped().await;

                // Clear logs for fresh start
                log_buffer.clear().await;

                // The run_manager_loop will restart automatically
                Response::Ok
            } else {
                Response::Error {
                    message: "No configuration available for restart".to_string(),
                }
            }
        }
        Request::Start { .. } => {
            // Daemon is already running with a config, can't start another
            Response::AlreadyRunning
        }
        Request::Logs { follow, lines } => {
            if follow {
                // Stream logs - send existing lines first, then subscribe
                let existing = log_buffer.get_lines(lines).await;
                for line in existing {
                    let resp = Response::LogLine(line);
                    let json = serde_json::to_string(&resp)? + "\n";
                    writer.write_all(json.as_bytes()).await?;
                }

                // Now stream new lines
                let mut rx = log_buffer.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(line) => {
                            let resp = Response::LogLine(line);
                            let json = serde_json::to_string(&resp)? + "\n";
                            if writer.write_all(json.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
                return Ok(());
            } else {
                let lines = log_buffer.get_lines(lines).await;
                Response::Logs {
                    lines,
                    complete: true,
                }
            }
        }
    };

    let json = serde_json::to_string(&response)? + "\n";
    writer.write_all(json.as_bytes()).await?;

    Ok(())
}

fn stop_container(container_id: &str) -> Result<()> {
    info!("Stopping container: {}", container_id);
    let _ = Command::new("docker")
        .arg("stop")
        .arg(container_id)
        .output();
    let _ = Command::new("docker").arg("rm").arg(container_id).output();
    Ok(())
}

async fn run_manager_loop(
    config: RunConfig,
    state: Arc<DaemonState>,
    log_buffer: Arc<LogBuffer>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    // Load env file to get RUN_ID
    load_and_apply_env_file(&config.env_file)?;
    let run_id = get_env_var("RUN_ID")?;

    let entrypoint_config = config.entrypoint.clone().map(|ep| Entrypoint {
        entrypoint: ep,
        args: config.entrypoint_args.clone(),
    });

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let run_mgr = RunManager::new(
            config.coordinator_program_id.clone(),
            config.env_file.clone(),
            config.local,
        )?;

        // Prepare image
        let docker_tag = match run_mgr.prepare_image().await {
            Ok(tag) => tag,
            Err(e) => {
                log_buffer
                    .push(format!("Error preparing image: {}", e))
                    .await;
                error!("Error preparing image: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                continue;
            }
        };

        state.update_image(docker_tag.clone()).await;
        log_buffer
            .push(format!("Using image: {}", docker_tag))
            .await;

        // Run container
        let container_id = match run_mgr.run_container(&docker_tag, &entrypoint_config) {
            Ok(id) => id,
            Err(e) => {
                log_buffer
                    .push(format!("Error starting container: {}", e))
                    .await;
                error!("Error starting container: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                continue;
            }
        };

        let container_info = ContainerInfo {
            container_id: container_id.clone(),
            image: docker_tag.clone(),
            run_id: run_id.clone(),
        };
        state.set_running(container_info, config.clone()).await;

        log_buffer
            .push(format!("Started container: {}", container_id))
            .await;

        // Stream logs with callback to buffer
        let _ = stream_logs_to_buffer(&container_id, log_buffer.clone(), &mut shutdown_rx).await;

        // Wait for container
        let exit_code = match run_mgr.wait_for_container(&container_id) {
            Ok(code) => code,
            Err(e) => {
                log_buffer
                    .push(format!("Error waiting for container: {}", e))
                    .await;
                error!("Error waiting for container: {}", e);
                -1
            }
        };

        log_buffer
            .push(format!("Container exited with code: {}", exit_code))
            .await;

        // Cleanup container
        let _ = run_mgr.stop_and_remove_container(&container_id);
        state.set_stopped().await;

        // Check shutdown
        if *shutdown_rx.borrow() || state.is_shutdown_requested().await {
            break;
        }

        // Only retry on version mismatch
        if exit_code == VERSION_MISMATCH_EXIT_CODE {
            log_buffer
                .push("Version mismatch, retrying...".to_string())
                .await;
            warn!("Version mismatch detected, re-checking coordinator for new version...");
            tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
        } else {
            log_buffer
                .push(format!(
                    "Container exited with code {}, shutting down daemon",
                    exit_code
                ))
                .await;
            break;
        }
    }

    Ok(())
}

async fn stream_logs_to_buffer(
    container_id: &str,
    log_buffer: Arc<LogBuffer>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<()> {
    let mut child = tokio::process::Command::new("docker")
        .arg("logs")
        .arg("-f")
        .arg(container_id)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start docker logs")?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let log_buffer_stdout = log_buffer.clone();
    let stdout_handle = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let reader = TokioBufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_buffer_stdout.push(line).await;
            }
        }
    });

    let log_buffer_stderr = log_buffer.clone();
    let stderr_handle = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let reader = TokioBufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_buffer_stderr.push(line).await;
            }
        }
    });

    tokio::select! {
        _ = child.wait() => {}
        _ = shutdown_rx.changed() => {
            child.kill().await?;
        }
    }

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    Ok(())
}
