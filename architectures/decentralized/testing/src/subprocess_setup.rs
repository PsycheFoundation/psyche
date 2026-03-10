use std::path::{Path, PathBuf};
use std::time::Duration;

use psyche_core::IntegrationTestLogMarker;
use tokio::process::{Child, Command};
use tokio::signal;

use crate::subprocess_watcher::{SubprocessWatcher, WatcherError};
use crate::utils::ConfigBuilder;

pub const CLIENT_PROCESS_PREFIX: &str = "client";
pub const VALIDATOR_PROCESS_NAME: &str = "validator";

const RPC_URL: &str = "http://127.0.0.1:8899";
const WS_RPC_URL: &str = "ws://127.0.0.1:8900";

/// Kill any stale test processes from prior runs to avoid port conflicts.
fn kill_stale_processes() {
    println!("[+] Cleaning up stale test processes...");
    // Best-effort — if processes don't exist, that's fine
    let _ = std::process::Command::new("pkill")
        .args(["-f", "solana-test-validator"])
        .output();
    let _ = std::process::Command::new("pkill")
        .args(["-f", "psyche-solana-client"])
        .output();
    // Brief pause to let ports be released
    std::thread::sleep(std::time::Duration::from_millis(500));
}

/// Holds all child processes and cleans them up on drop.
/// The Child handles MUST be stored here because kill_on_drop(true) will
/// kill the process when the Child is dropped.
pub struct SubprocessTestCleanup {
    validator: Option<Child>,
    clients: Vec<Child>,
}

impl SubprocessTestCleanup {
    /// Store a client child process so it stays alive and gets cleaned up on drop.
    pub fn add_client(&mut self, child: Child) {
        self.clients.push(child);
    }
}

impl Drop for SubprocessTestCleanup {
    fn drop(&mut self) {
        println!("\nCleaning up subprocesses...");
        // Kill all client processes via signal (kill_on_drop handles the rest)
        for child in &mut self.clients {
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        // Kill validator
        if let Some(ref mut validator) = self.validator {
            if let Some(pid) = validator.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
    }
}

/// Run a command and panic if it fails. Returns stdout as string.
async fn run_cmd(program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .unwrap_or_else(|e| panic!("Failed to run {program}: {e}"));
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("{program} failed:\nstdout: {stdout}\nstderr: {stderr}");
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Get the path to a program artifact. Checks SOLANA_PROGRAMS_DIR env var first,
/// then falls back to the local anchor build output.
fn program_artifact_path(env_var: &str, fallback_relative: &str, filename: &str) -> PathBuf {
    if let Ok(dir) = std::env::var(env_var) {
        PathBuf::from(dir).join(filename)
    } else {
        PathBuf::from(fallback_relative).join(filename)
    }
}

fn coordinator_so_path() -> PathBuf {
    program_artifact_path(
        "SOLANA_PROGRAMS_DIR",
        "../solana-coordinator/target/deploy",
        "psyche_solana_coordinator.so",
    )
}

fn coordinator_keypair_path() -> PathBuf {
    program_artifact_path(
        "SOLANA_PROGRAMS_DIR",
        "../solana-coordinator/target/deploy",
        "psyche_solana_coordinator-keypair.json",
    )
}

fn authorizer_so_path() -> PathBuf {
    program_artifact_path(
        "SOLANA_AUTHORIZER_DIR",
        "../solana-authorizer/target/deploy",
        "psyche_solana_authorizer.so",
    )
}

fn authorizer_keypair_path() -> PathBuf {
    program_artifact_path(
        "SOLANA_AUTHORIZER_DIR",
        "../solana-authorizer/target/deploy",
        "psyche_solana_authorizer-keypair.json",
    )
}

fn authorizer_idl_path() -> PathBuf {
    program_artifact_path(
        "SOLANA_AUTHORIZER_DIR",
        "../solana-authorizer/target/deploy",
        "psyche_solana_authorizer.json",
    )
}

fn test_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("TEST_CONFIG_PATH") {
        PathBuf::from(path)
    } else {
        PathBuf::from("../../../config/solana-test/test-config.toml")
    }
}

/// Start solana-test-validator and wait for it to be ready.
/// Registers the validator PID in the watcher registry for chaos actions.
async fn start_validator(watcher: &SubprocessWatcher) -> Child {
    println!("[+] Generating validator keypair...");
    run_cmd(
        "solana-keygen",
        &["new", "--no-bip39-passphrase", "--force"],
    )
    .await;

    println!("[+] Setting solana config...");
    run_cmd("solana", &["config", "set", "--url", "localhost"]).await;

    println!("[+] Starting solana-test-validator...");
    let child = Command::new("solana-test-validator")
        .arg("-r")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("Failed to start solana-test-validator");

    // Register validator PID in the watcher registry (for chaos actions)
    if let Some(pid) = child.id() {
        let mut registry = watcher.registry.lock().await;
        registry.register(VALIDATOR_PROCESS_NAME.to_string(), pid);
        println!("[+] Validator started (pid {pid})");
    }

    // Wait for validator to be ready
    println!("[+] Waiting for validator to be ready...");
    for attempt in 0..60 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = Command::new("solana")
            .args(["cluster-version", "--url", RPC_URL])
            .output()
            .await;
        if let Ok(output) = result {
            if output.status.success() {
                println!("[+] Validator ready after {}s", attempt + 1);
                break;
            }
        }
        if attempt == 59 {
            panic!("Validator failed to start after 60 seconds");
        }
    }

    child
}

/// Deploy the Solana programs (authorizer + coordinator) and init the authorizer IDL.
async fn deploy_programs() {
    println!("[+] Deploying Solana Authorizer...");
    let auth_so = authorizer_so_path();
    let auth_keypair = authorizer_keypair_path();
    let auth_idl = authorizer_idl_path();

    assert!(
        auth_so.exists(),
        "Authorizer .so not found at {}",
        auth_so.display()
    );

    run_cmd(
        "solana",
        &[
            "program",
            "deploy",
            &auth_so.to_string_lossy(),
            "--program-id",
            &auth_keypair.to_string_lossy(),
            "--url",
            RPC_URL,
            "--max-len",
            "500000",
        ],
    )
    .await;

    // Get authorizer program ID for IDL init
    let auth_id = run_cmd(
        "solana",
        &["address", "-k", &auth_keypair.to_string_lossy()],
    )
    .await;
    let auth_id = auth_id.trim();
    println!("[+] Authorizer program ID: {auth_id}");

    // IDL init
    println!("[+] Initializing Authorizer IDL...");
    run_cmd(
        "anchor",
        &[
            "idl",
            "init",
            "--provider.cluster",
            RPC_URL,
            "--provider.wallet",
            &format!(
                "{}/.config/solana/id.json",
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
            ),
            "--filepath",
            &auth_idl.to_string_lossy(),
            auth_id,
        ],
    )
    .await;

    println!("[+] Deploying Solana Coordinator...");
    let coord_so = coordinator_so_path();
    let coord_keypair = coordinator_keypair_path();

    assert!(
        coord_so.exists(),
        "Coordinator .so not found at {}",
        coord_so.display()
    );

    run_cmd(
        "solana",
        &[
            "program",
            "deploy",
            &coord_so.to_string_lossy(),
            "--program-id",
            &coord_keypair.to_string_lossy(),
            "--url",
            RPC_URL,
            "--max-len",
            "500000",
        ],
    )
    .await;

    println!("[+] Programs deployed successfully!");
}

/// Run the setup-test-run logic: wallet, airdrop, create-run, update-config, unpause.
/// This replaces scripts/setup-test-run.sh.
async fn setup_test_run(_min_clients: usize, owner_keypair_path: Option<&Path>) -> Option<PathBuf> {
    let (wallet_path, cleanup_path) = if let Some(path) = owner_keypair_path {
        (path.to_path_buf(), None)
    } else {
        let tmp = PathBuf::from(format!("/tmp/test-owner-{}.json", std::process::id()));
        run_cmd(
            "solana-keygen",
            &[
                "new",
                "--no-bip39-passphrase",
                "--force",
                "--outfile",
                &tmp.to_string_lossy(),
            ],
        )
        .await;
        let cleanup = tmp.clone();
        (tmp, Some(cleanup))
    };

    println!("[+] Airdropping SOL to wallet...");
    let pubkey = run_cmd("solana-keygen", &["pubkey", &wallet_path.to_string_lossy()]).await;
    run_cmd(
        "solana",
        &["airdrop", "10", pubkey.trim(), "--url", RPC_URL],
    )
    .await;

    println!("[+] Creating join authorization...");
    run_cmd(
        "run-manager",
        &[
            "join-authorization-create",
            "--wallet-private-key-path",
            &wallet_path.to_string_lossy(),
            "--rpc",
            RPC_URL,
            "--authorizer",
            "11111111111111111111111111111111",
        ],
    )
    .await;

    println!("[+] Creating run...");
    run_cmd(
        "run-manager",
        &[
            "create-run",
            "--wallet-private-key-path",
            &wallet_path.to_string_lossy(),
            "--rpc",
            RPC_URL,
            "--ws-rpc",
            WS_RPC_URL,
            "--run-id",
            "test",
            "--client-version",
            "latest",
        ],
    )
    .await;

    let config_path = test_config_path();
    println!("[+] Updating config from {}...", config_path.display());
    run_cmd(
        "run-manager",
        &[
            "update-config",
            "--wallet-private-key-path",
            &wallet_path.to_string_lossy(),
            "--rpc",
            RPC_URL,
            "--ws-rpc",
            WS_RPC_URL,
            "--run-id",
            "test",
            "--config-path",
            &config_path.to_string_lossy(),
        ],
    )
    .await;

    println!("[+] Unpausing run...");
    run_cmd(
        "run-manager",
        &[
            "set-paused",
            "--wallet-private-key-path",
            &wallet_path.to_string_lossy(),
            "--rpc",
            RPC_URL,
            "--ws-rpc",
            WS_RPC_URL,
            "--run-id",
            "test",
            "--resume",
        ],
    )
    .await;

    println!("[+] Test run setup complete!");
    cleanup_path
}

/// Spawn a client process. The Child handle is stored in `cleanup` automatically
/// so it stays alive and gets cleaned up on drop.
pub async fn spawn_client(
    cleanup: &mut SubprocessTestCleanup,
    client_index: usize,
    keypair_path: Option<&Path>,
    watcher: &SubprocessWatcher,
    filters: Vec<IntegrationTestLogMarker>,
) -> Result<String, WatcherError> {
    let name = format!("{CLIENT_PROCESS_PREFIX}-{client_index}");

    // Generate or use provided wallet
    let wallet_path = if let Some(path) = keypair_path {
        path.to_path_buf()
    } else {
        let tmp = PathBuf::from(format!(
            "/tmp/test-client-{}-{}.json",
            client_index,
            std::process::id()
        ));
        run_cmd(
            "solana-keygen",
            &[
                "new",
                "--no-bip39-passphrase",
                "--force",
                "--outfile",
                &tmp.to_string_lossy(),
            ],
        )
        .await;
        tmp
    };

    // Airdrop SOL to the client wallet
    let pubkey = run_cmd("solana-keygen", &["pubkey", &wallet_path.to_string_lossy()]).await;
    run_cmd(
        "solana",
        &["airdrop", "10", pubkey.trim(), "--url", RPC_URL],
    )
    .await;

    // Spawn psyche-solana-client train
    let mut child = Command::new("psyche-solana-client")
        .args([
            "train",
            "--wallet-private-key-path",
            &wallet_path.to_string_lossy(),
            "--rpc",
            RPC_URL,
            "--ws-rpc",
            WS_RPC_URL,
            "--run-id",
            "test",
            "--logs",
            "json",
        ])
        .env("BLOBS_GC_INTERVAL_MILLIS", "500")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("Failed to spawn psyche-solana-client");

    let pid = child.id().expect("Failed to get child PID");
    let stdout = child.stdout.take().expect("Failed to take child stdout");

    // Register in watcher for health checks / signals
    {
        let mut registry = watcher.registry.lock().await;
        registry.register(name.clone(), pid);
    }

    // Start monitoring the process stdout
    watcher.monitor_process(&name, stdout, filters);

    println!("[+] Spawned client {name} (pid {pid})");
    cleanup.add_client(child);
    Ok(name)
}

/// Full e2e test setup: start validator, deploy, setup run, spawn clients.
pub async fn e2e_testing_setup(
    watcher: &SubprocessWatcher,
    init_num_clients: usize,
) -> SubprocessTestCleanup {
    e2e_testing_setup_with_min(watcher, init_num_clients, init_num_clients, None).await
}

/// Full e2e test setup with explicit min_clients and optional owner keypair.
pub async fn e2e_testing_setup_with_min(
    watcher: &SubprocessWatcher,
    init_num_clients: usize,
    min_clients: usize,
    owner_keypair_path: Option<&Path>,
) -> SubprocessTestCleanup {
    // Write the test config
    #[cfg(not(feature = "python"))]
    let _config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_min_clients(min_clients)
        .build();
    #[cfg(feature = "python")]
    let _config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_min_clients(min_clients)
        .with_architecture("HfAuto")
        .with_batch_size(8 * std::cmp::max(init_num_clients, 1) as u32)
        .build();

    // Kill any stale processes from prior runs
    kill_stale_processes();

    // Start validator
    let validator = start_validator(watcher).await;

    // Deploy programs
    deploy_programs().await;

    // Setup the run (wallet, airdrop, create-run, update-config, unpause)
    let _cleanup_wallet = setup_test_run(min_clients, owner_keypair_path).await;

    let mut cleanup = SubprocessTestCleanup {
        validator: Some(validator),
        clients: Vec::new(),
    };

    // Spawn initial clients
    for i in 1..=init_num_clients {
        let name = spawn_client(
            &mut cleanup,
            i,
            None,
            watcher,
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .await
        .unwrap();
        println!("[+] Client {name} started");
    }

    spawn_ctrl_c_task();

    cleanup
}

/// Kill all tracked client processes.
pub async fn kill_all_clients(watcher: &SubprocessWatcher, signal: i32) {
    let registry = watcher.registry.lock().await;
    let names = registry.running_names();
    for name in &names {
        if name.starts_with(CLIENT_PROCESS_PREFIX) {
            if let Some(pid) = registry.get_pid(name) {
                println!("Killing process {name} (pid {pid})");
                unsafe {
                    libc::kill(pid as i32, signal);
                }
            }
        }
    }
    drop(registry);

    // Small delay to ensure processes terminate
    tokio::time::sleep(Duration::from_secs(2)).await;
}

fn spawn_ctrl_c_task() {
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        println!("\nCtrl+C received. Exiting...");
        std::process::exit(0);
    });
}
