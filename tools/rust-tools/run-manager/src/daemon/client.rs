use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use super::protocol::{Request, Response, get_socket_path};

/// Send a request to the daemon and get a response
fn send_request(request: &Request) -> Result<Response> {
    let socket_path = get_socket_path();
    let stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon. Is it running?")?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;

    let mut stream = BufReader::new(stream);

    // Send request
    let json = serde_json::to_string(request)? + "\n";
    stream.get_mut().write_all(json.as_bytes())?;

    // Read response
    let mut line = String::new();
    stream.read_line(&mut line)?;
    let response: Response = serde_json::from_str(&line)?;

    Ok(response)
}

/// Check if daemon is running by attempting to connect
pub fn is_daemon_running() -> bool {
    let socket_path = get_socket_path();
    UnixStream::connect(&socket_path).is_ok()
}

/// Send stop command to daemon
pub fn send_stop() -> Result<()> {
    match send_request(&Request::Stop) {
        Ok(Response::Stopped) => {
            println!("Daemon stopped");
            Ok(())
        }
        Ok(Response::Error { message }) => Err(anyhow!("Error: {}", message)),
        Ok(_) => Err(anyhow!("Unexpected response")),
        Err(e) => {
            if e.to_string().contains("Failed to connect") {
                println!("Daemon is not running");
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Send restart command to daemon
pub fn send_restart() -> Result<()> {
    match send_request(&Request::Restart) {
        Ok(Response::Ok) => {
            println!("Daemon restarting...");
            Ok(())
        }
        Ok(Response::Error { message }) => Err(anyhow!("Error: {}", message)),
        Ok(_) => Err(anyhow!("Unexpected response")),
        Err(e) => {
            if e.to_string().contains("Failed to connect") {
                Err(anyhow!("Daemon is not running"))
            } else {
                Err(e)
            }
        }
    }
}

/// Get daemon status
pub fn get_status() -> Result<()> {
    match send_request(&Request::Status) {
        Ok(Response::Status {
            running,
            container_id,
            image,
            uptime_secs,
            run_id,
        }) => {
            let status = serde_json::json!({
                "daemon": "running",
                "container": {
                    "running": running,
                    "id": container_id,
                    "image": image,
                    "uptime_secs": uptime_secs,
                    "run_id": run_id,
                }
            });
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
        Ok(Response::Error { message }) => Err(anyhow!("Error: {}", message)),
        Ok(_) => Err(anyhow!("Unexpected response")),
        Err(e) => {
            if e.to_string().contains("Failed to connect") {
                let status = serde_json::json!({
                    "daemon": "not running"
                });
                println!("{}", serde_json::to_string_pretty(&status)?);
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Get logs from daemon
pub fn get_logs(follow: bool, lines: Option<usize>) -> Result<()> {
    let socket_path = get_socket_path();
    let stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon. Is it running?")?;

    if follow {
        // No timeout for streaming
        stream.set_read_timeout(None)?;
    } else {
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    }

    let mut stream = BufReader::new(stream);

    // Send request
    let request = Request::Logs { follow, lines };
    let json = serde_json::to_string(&request)? + "\n";
    stream.get_mut().write_all(json.as_bytes())?;

    if follow {
        // Stream mode - read lines until connection closes
        loop {
            let mut line = String::new();
            match stream.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let response: Response = serde_json::from_str(&line)?;
                    match response {
                        Response::LogLine(log_line) => {
                            println!("{}", log_line);
                        }
                        Response::Error { message } => {
                            eprintln!("Error: {}", message);
                            break;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        break;
                    }
                }
            }
        }
    } else {
        // Batch mode - read single response
        let mut line = String::new();
        stream.read_line(&mut line)?;
        let response: Response = serde_json::from_str(&line)?;

        match response {
            Response::Logs { lines, .. } => {
                for line in lines {
                    println!("{}", line);
                }
            }
            Response::Error { message } => {
                return Err(anyhow!("Error: {}", message));
            }
            _ => {
                return Err(anyhow!("Unexpected response"));
            }
        }
    }

    Ok(())
}

/// Start the daemon (via fork)
pub fn start_daemon(
    env_file: PathBuf,
    coordinator_program_id: String,
    local: bool,
    entrypoint: Option<String>,
    entrypoint_args: Vec<String>,
) -> Result<()> {
    super::server::start_daemon(
        env_file,
        coordinator_program_id,
        local,
        entrypoint,
        entrypoint_args,
    )
}
