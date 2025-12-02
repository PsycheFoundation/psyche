use anyhow::Result;
use clap::Parser;
use run_manager::run_manager::RunManager;
use std::path::PathBuf;
use tracing::error;

#[derive(Parser, Debug)]
#[command(name = "run-manager")]
#[command(about = "Manager to download client containers based on a run version")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to .env file with environment variables
    #[arg(long, global = true)]
    env_file: PathBuf,

    /// Coordinator program ID
    #[arg(
        long,
        global = true,
        default_value = "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y"
    )]
    coordinator_program_id: String,

    /// Use a local Docker image instead of pulling from registry.
    #[arg(long, global = true)]
    local: bool,
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

    let run_mgr = RunManager::new(args.coordinator_program_id, args.env_file, args.local)?;
    let result = run_mgr.run().await;
    if let Err(e) = &result {
        error!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
