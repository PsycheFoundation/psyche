use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser};
use rand::seq::SliceRandom;
use serde::Deserialize;
use std::ffi::OsString;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use time::OffsetDateTime;
use time::macros::format_description;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)] // it's only used for generating the docs correctly.
#[derive(Parser, Debug)]
enum Commands {
    /// Starts the local-testnet running each part of the system in a separate terminal pane.
    Start {
        #[command(flatten)]
        start_args: StartArgs,
    },
    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[derive(Parser, Debug, Clone)]
struct StartArgs {
    /// Number of clients to start
    #[clap(long, value_parser = validate_num_clients)]
    num_clients: usize,

    /// File path to the configuration that the coordinator will need to start.
    #[clap(long,value_parser = validate_config_path)]
    config_path: PathBuf,

    /// If provided, write DisTrO data to disk in this path.
    #[clap(long)]
    write_distro_data: Option<PathBuf>,

    /// Port where the server for this testnet will be listen it to (this is the one that clients must use when connecting).
    #[clap(long, default_value_t = 20000)]
    server_port: u16,

    /// Enables a terminal-based graphical interface for monitoring analytics.
    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false,
        env
    )]
    tui: bool,

    /// Kill N clients randomly every <RANDOM_KILL_INTERVAL> seconds
    #[clap(long)]
    random_kill_num: Option<usize>,

    /// Which clients we're allowed to kill randomly
    #[clap(long, value_delimiter = ',', default_values_t = &[])]
    allowed_to_kill: Vec<usize>,

    #[clap(long, default_value_t = 120)]
    /// Kill <RANDOM_KILL_NUM> clients randomly every N seconds
    random_kill_interval: u64,

    /// Sets the level of the logging for more granular information
    #[clap(long, default_value = "warn,psyche=debug")]
    log: String,

    /// HF repo where the first client could get the model and the configuration to use.
    #[clap(long)]
    first_client_checkpoint: Option<String>,

    // HF token for all the clients to fetch the model at the beggining of the run.
    #[clap(long)]
    hf_token: Option<String>,

    #[clap(long, default_value_t = false)]
    write_log: bool,

    #[clap(long, env)]
    wandb_project: Option<String>,

    #[clap(long, env)]
    wandb_group: Option<String>,

    #[clap(long, env)]
    wandb_entity: Option<String>,

    #[clap(long, env)]
    optim_stats: Option<u32>,

    #[clap(long, env)]
    eval_tasks: Option<String>,

    /// Each client writes events to a subdir of this path, named after its node ID.
    /// Pass this dir to `observer --events-dir` to inspect the run.
    #[clap(long)]
    events_dir: Option<PathBuf>,
}

fn validate_num_clients(s: &str) -> Result<usize> {
    let n: usize = s
        .parse()
        .context("NUM_CLIENTS must be a positive integer")?;
    if n > 0 {
        Ok(n)
    } else {
        bail!("NUM_CLIENTS must be a positive integer")
    }
}

fn validate_config_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("Config path {s} does not exist"))
    }
}

#[derive(Deserialize)]
struct TomlWithRunId {
    run_id: String,
}

fn extract_run_id(state_path: &PathBuf) -> Result<String> {
    let toml: TomlWithRunId = toml::from_str(&std::fs::read_to_string(state_path)?)?;
    Ok(toml.run_id)
}

fn main() -> Result<()> {
    #[cfg(feature = "python")]
    psyche_python_extension_impl::init_embedded_python()?;

    let args = Args::parse();
    let command = args.command;

    match command {
        Commands::Start { start_args } => {
            if let Some(n_kill) = start_args.random_kill_num {
                if n_kill > start_args.num_clients {
                    bail!(
                        "You requested to kill {n_kill} clients randomly, but you only have {} clients.",
                        start_args.num_clients
                    );
                }
            }
            let state_path = start_args.config_path.join("state.toml");
            let data_path = start_args.config_path.join("data.toml");

            println!("{start_args:?}");

            // Pre-build packages
            Command::new("cargo")
                .args(["build", "-p", "psyche-centralized-server"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to build server");

            Command::new("cargo")
                .args(["build", "-p", "psyche-centralized-client"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to build client");

            let validate_cmd = if data_path.exists() {
                vec![
                    "run",
                    "-p",
                    "psyche-centralized-server",
                    "validate-config",
                    "--state",
                    state_path.to_str().unwrap(),
                    "--data-config",
                    data_path.to_str().unwrap(),
                ]
            } else {
                vec![
                    "run",
                    "-p",
                    "psyche-centralized-server",
                    "validate-config",
                    "--state",
                    state_path.to_str().unwrap(),
                ]
            };
            // Validate config
            Command::new("cargo")
                .args(validate_cmd)
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to validate config");

            let run_id = extract_run_id(&state_path)?;

            // Create tmux session
            Command::new("tmux")
                .args(["new-session", "-d", "-s", "psyche"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to create tmux session");

            // Split windows and set up panes
            Command::new("tmux")
                .args(["split-window", "-h"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to split window horizontally");

            Command::new("tmux")
                .args(["select-pane", "-t", "0"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to select pane");

            Command::new("tmux")
                .args(["split-window", "-v"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to split window vertically");

            // Split remaining panes for clients
            Command::new("tmux")
                .args(["select-pane", "-t", "2"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to select pane");

            for _ in 1..start_args.num_clients {
                Command::new("tmux")
                    .args(["split-window", "-v"])
                    .status()
                    .ok()
                    .and_then(|s| s.success().then_some(()))
                    .expect("Failed to split window for client");
            }

            let start_time = OffsetDateTime::now_utc();

            // Start server
            let mut server_cmd = format!(
                "RUST_LOG={} cargo run -p psyche-centralized-server run --state {} --server-port {} --tui {}",
                start_args.log,
                state_path.display(),
                start_args.server_port,
                start_args.tui
            );
            if data_path.exists() {
                server_cmd.push_str(&format!(" --data-config {}", data_path.display()));
            }

            if let Some(dir) = &start_args.events_dir {
                server_cmd.push_str(&format!(" --events-dir {}", dir.display()));
            }

            println!("starting server: {server_cmd:?}");

            Command::new("tmux")
                .args(["select-pane", "-t", "0"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to select server pane");

            Command::new("tmux")
                .args(["send-keys", &server_cmd, "C-m"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to send server command");

            println!("Waiting for server startup...");
            loop {
                if TcpStream::connect(format!("127.0.0.1:{}", start_args.server_port)).is_ok() {
                    println!("Server started!");
                    break;
                }
            }

            // Start nvtop
            Command::new("tmux")
                .args(["select-pane", "-t", "1"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to select nvtop pane");

            Command::new("tmux")
                .args(["send-keys", "nvtop", "C-m"])
                .status()
                .ok()
                .and_then(|s| s.success().then_some(()))
                .expect("Failed to start nvtop");

            // Start clients
            for i in 2..=start_args.num_clients + 1 {
                start_client(&start_args, i, &run_id, true, start_time);
            }

            // // Attach to tmux session
            let mut tmux_session = Command::new("tmux")
                .args(["attach-session", "-t", "psyche"])
                .spawn()?;

            if let Some(kill_num) = start_args.random_kill_num {
                let allowed_to_kill = |item: &usize| {
                    if start_args.allowed_to_kill.is_empty() {
                        true
                    } else {
                        start_args.allowed_to_kill.contains(&(item - 1))
                    }
                };
                let mut last_kill_time = Instant::now();
                let kill_interval = Duration::from_secs(start_args.random_kill_interval);
                loop {
                    std::thread::sleep(Duration::from_millis(500));
                    if Instant::now() > (last_kill_time + kill_interval) {
                        last_kill_time = Instant::now();

                        let to_kill = {
                            let mut client_nums: Vec<usize> = (2..=start_args.num_clients + 1)
                                .filter(allowed_to_kill)
                                .collect();

                            client_nums.shuffle(&mut rand::rng());

                            client_nums.truncate(kill_num);
                            client_nums
                        };
                        for kill in to_kill {
                            Command::new("tmux")
                                .args(["select-pane", "-t", &kill.to_string()])
                                .status()
                                .ok()
                                .and_then(|s| s.success().then_some(()))
                                .expect("Failed to select client pane");
                            // send ctrl-c
                            Command::new("tmux")
                                .args(["send-keys", "-t", &kill.to_string(), "C-c"])
                                .status()
                                .ok()
                                .and_then(|s| s.success().then_some(()))
                                .expect("Failed to kill client");
                            // restart client
                            start_client(&start_args, kill, &run_id, false, start_time);
                        }
                    }

                    if tmux_session.try_wait().unwrap().is_some() {
                        break;
                    }
                }
            }

            let _ = tmux_session.wait(); // to prevent weird async tmux overlap with normal shell

            // failsafe kill
            Command::new("tmux")
                .args(["kill-session", "-t", "psyche"])
                .status()
                .expect("Failed to kill tmux session");

            Ok(())
        }
        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<Args>();

            Ok(())
        }
    }
}

fn start_client(
    args: &StartArgs,
    i: usize,
    run_id: &String,
    print: bool,
    start_time: OffsetDateTime,
) {
    // hex 1, 2, 3, etc.
    let raw_key = format!("{:0>64x}", i - 1);

    Command::new("tmux")
        .args(["select-pane", "-t", &i.to_string()])
        .status()
        .ok()
        .and_then(|s| s.success().then_some(()))
        .expect("Failed to select client pane");

    let mut cmd: OsString = if let Some(token) = &args.hf_token {
        format!("HF_TOKEN={token} ").into()
    } else {
        OsString::new()
    };

    let metrics_local_port = 6269 + i - 1;

    cmd.push(format!(
        "METRICS_LOCAL_PORT={metrics_local_port} RUST_LOG={} RUST_BACKTRACE=1 RAW_IDENTITY_SECRET_KEY={} cargo run -p psyche-centralized-client train --run-id {} --server-addr localhost:{} --logs {}",
        args.log,
        raw_key,
        run_id,
        args.server_port,
        if args.tui {
            "tui"
        } else {
            "console"
        }
    ));

    if let Some(dir) = &args.write_distro_data {
        cmd.push(" --write-gradients-dir ");
        cmd.push(dir);
    }

    if let Some(repo) = &args.first_client_checkpoint {
        if i == 2 {
            cmd.push(format!(" --checkpoint-dir ./checkpoints --hub-repo {repo}"));
        }
    }

    if let Some(entity) = &args.wandb_entity {
        cmd.push(format!(" --wandb-entity {entity}"));
    }
    if let Some(group) = &args.wandb_group {
        cmd.push(format!(" --wandb-group {group}"));
    }
    if let Some(project) = &args.wandb_project {
        cmd.push(format!(" --wandb-project {project}"));
    }

    if args.write_log {
        let log_dir = format!(
            "./logs/{}",
            start_time
                .format(format_description!(
                    "[year]-[month]-[day]_[hour]:[minute]:[second]"
                ))
                .unwrap()
        );
        std::fs::create_dir_all(&log_dir).unwrap();
        cmd.push(format!(" --write-log {log_dir}/client-{}.txt", i - 1))
    }

    if let Some(s) = args.optim_stats {
        cmd.push(format!(" --optim-stats {s}"));
    }

    if let Some(evals) = &args.eval_tasks {
        cmd.push(format!(" --eval-tasks {evals}"))
    }

    if let Some(dir) = &args.events_dir {
        cmd.push(" --events-dir ");
        cmd.push(dir);
    }

    if print {
        println!("starting client {i}: {cmd:?}");
    }

    Command::new("tmux")
        .args([OsString::from("send-keys"), cmd, OsString::from("C-m")])
        .status()
        .ok()
        .and_then(|s| s.success().then_some(()))
        .expect("Failed to send server command");
}
