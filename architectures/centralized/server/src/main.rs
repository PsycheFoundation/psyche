mod app;
mod dashboard;

use anyhow::{Context, Result};
use app::{App, DataServerInfo};
use clap::{ArgAction, Parser};
use psyche_centralized_shared::ClientId;
use psyche_coordinator::Coordinator;
use psyche_tui::LogOutput;
use std::path::{Path, PathBuf};
use tracing::{error, info, Level};

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Checks that the configuration declared in the `state.toml` file is valid.
    ValidateConfig {
        /// Path to the `state.toml` file to validate.
        #[clap(long)]
        state: PathBuf,
        /// Path to `data.toml` file to validate. If no provided then it will not be checked.
        #[clap(long)]
        data_config: Option<PathBuf>,
    },
    /// Starts the server and launches the coordinator with the declared configuration.
    Run {
        #[command(flatten)]
        run_args: RunArgs,
    },
    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[derive(Parser, Debug, Clone)]
struct RunArgs {
    /// Path to TOML of Coordinator state
    #[clap(long)]
    state: PathBuf,

    /// Port for the server, which clients will use to connect. if not specified, a random free port will be chosen.
    #[clap(short, long)]
    server_port: Option<u16>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    tui: bool,

    /// Path to TOML of data server config
    #[clap(long)]
    data_config: Option<PathBuf>,

    /// Path to save the server and coordinator state.
    #[clap(long)]
    save_state_dir: Option<PathBuf>,

    /// Sets the warmup time for the run. This overrides the `warmup_time` declared in the state file.
    #[clap(long)]
    init_warmup_time: Option<u64>,

    /// Sets the minimum number of clients required to start a run. This overrides the `min_clients` declared in the state file.
    #[clap(long)]
    init_min_clients: Option<u16>,

    /// Automatically withdraw clients that disconenct from the server
    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    withdraw_on_disconnect: bool,
}

fn load_config_state(
    state_path: PathBuf,
    data_config_path: Option<PathBuf>,
) -> Result<(Coordinator<ClientId>, Option<DataServerInfo>)> {
    let coordinator: Coordinator<ClientId> = toml::from_str(std::str::from_utf8(
        &std::fs::read(&state_path).with_context(|| {
            format!(
                "failed to read coordinator state toml file {:?}",
                state_path
            )
        })?,
    )?)?;

    let data_server_config = match data_config_path {
        Some(config_path) => {
            let mut data_config: DataServerInfo = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path).with_context(|| {
                    format!("failed to read data server config toml file {config_path:?}")
                })?,
            )?)
            .with_context(|| {
                format!("failed to parse data server config toml file {config_path:?}")
            })?;

            // data dir, if relative, should be relative to the config's path.
            if !data_config.dir.is_absolute() {
                let config_dir = Path::new(&config_path).parent().unwrap_or(Path::new(""));
                data_config.dir = config_dir.join(data_config.dir);
            }
            Some(data_config)
        }
        None => None,
    };

    Ok((coordinator, data_server_config))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let command = args.command;
    match command {
        Commands::ValidateConfig {
            state: state_path,
            data_config: data_config_path,
        } => {
            let config = load_config_state(state_path.clone(), data_config_path);
            let _ = psyche_tui::init_logging(LogOutput::Console, Level::INFO, None, false, None);
            match config {
                Ok(_) => info!("Configs are OK!"),
                Err(error) => error!("Error found in config: {}", error),
            }
        }
        Commands::Run { run_args } => {
            let config = load_config_state(run_args.state, run_args.data_config);
            let logger = psyche_tui::init_logging(
                if run_args.tui {
                    LogOutput::TUI
                } else {
                    LogOutput::Console
                },
                Level::INFO,
                None,
                true,
                Some("centralized-server".to_string()),
            )?;
            match config {
                Ok(config) => {
                    App::new(
                        run_args.tui,
                        config.0,
                        config.1,
                        run_args.server_port,
                        run_args.save_state_dir,
                        run_args.init_warmup_time,
                        run_args.init_min_clients,
                        run_args.withdraw_on_disconnect,
                    )
                    .await?
                    .run()
                    .await?
                }
                Err(error) => error!("Error found in config: {}", error),
            }
            logger.shutdown()?;
        }
        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<Args>();

            return Ok(());
        }
    }

    Ok(())
}
