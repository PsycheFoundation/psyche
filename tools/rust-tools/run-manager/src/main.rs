use anyhow::{Result, bail};
use clap::Parser;
use run_manager::run_manager::{Entrypoint, RunManager};
use std::path::PathBuf;
use tracing::error;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("GIT_HASH");
const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

fn long_version() -> &'static str {
    Box::leak(
        format!("{}\ngit: {}\nbuilt: {}", VERSION, GIT_HASH, BUILD_TIMESTAMP).into_boxed_str(),
    )
}

#[derive(Parser, Debug)]
#[command(name = "run-manager", version = VERSION, long_version = long_version())]
#[command(
    about = "Manager to download Psyche client container based on a version specified in the run"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to .env file with environment variables
    #[arg(long)]
    env_file: PathBuf,

    /// Coordinator program ID
    #[arg(long, default_value = "4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7")]
    coordinator_program_id: String,

    /// Use a local Docker image instead of pulling from registry.
    /// This is only meant for testing purposes, since it is easier to
    /// check a version update when the two docker images are local. Do not
    /// use in production
    #[arg(long)]
    local: bool,

    /// Optional entrypoint
    #[arg(long)]
    entrypoint: Option<String>,

    /// Arguments to pass to the entrypoint (use after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    entrypoint_args: Vec<String>,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(Commands::PrintAllHelp { markdown }) = args.command {
        // This is a required argument for the time being.
        assert!(markdown);
        let () = clap_markdown::print_help_markdown::<Args>();
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let entrypoint = match args.entrypoint {
        Some(entrypoint) => Some(Entrypoint {
            entrypoint,
            args: args.entrypoint_args,
        }),
        None if !args.entrypoint_args.is_empty() => {
            bail!(
                "unexpected trailing arguments {:?}. did you mean to pass --entrypoint?",
                args.entrypoint_args
            );
        }
        None => None,
    };

    let run_mgr = RunManager::new(args.coordinator_program_id, args.env_file, args.local)?;

    let result = run_mgr.run(entrypoint).await;
    if let Err(e) = &result {
        error!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
