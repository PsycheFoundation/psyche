use anchor_client::{
    Cluster,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::{EncodableKey, Keypair},
    },
};
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use psyche_solana_rpc::SolanaBackend;
use run_manager::commands::{self, Command};
use run_manager::docker::manager::{Entrypoint, RunManager};
use run_manager::parse_optional_pubkey;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;

// Command parameter imports
use commands::authorization::{
    CommandJoinAuthorizationCreate, CommandJoinAuthorizationDelegate,
    CommandJoinAuthorizationDelete, CommandJoinAuthorizationRead,
};
use commands::can_join::CommandCanJoin;
use commands::run::{
    CommandCheckpoint, CommandCloseRun, CommandCreateRun, CommandDownloadResults,
    CommandJsonDumpRun, CommandJsonDumpUser, CommandSetFutureEpochRates, CommandSetPaused,
    CommandTick, CommandUpdateConfig, CommandUploadData,
};
use commands::treasury::{CommandTreasurerClaimRewards, CommandTreasurerTopUpRewards};
use run_manager::docker::coordinator_client::CoordinatorClient;
use run_manager::docker::{
    RunInfo, find_joinable_runs, parse_delegate_authorizer_from_env, parse_wallet_pubkey,
};

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
struct CliArgs {
    #[command(subcommand)]
    command: Option<Commands>,

    // Docker mode args (used when no subcommand is provided)
    /// Path to .env file with environment variables (Docker mode)
    #[arg(long)]
    env_file: Option<PathBuf>,

    /// Coordinator program ID (Docker mode)
    #[arg(long, default_value = "4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7")]
    coordinator_program_id: String,

    /// Use a local Docker image instead of pulling from registry (Docker mode)
    #[arg(long)]
    local: bool,

    /// Only join runs where this pubkey is the join_authority (Docker mode)
    #[arg(long)]
    authorizer: Option<String>,

    /// Optional entrypoint (Docker mode)
    #[arg(long)]
    entrypoint: Option<String>,

    /// Arguments to pass to the entrypoint (use after --) (Docker mode)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    entrypoint_args: Vec<String>,
}

#[derive(Args, Debug, Clone)]
struct WalletArgs {
    #[clap(short, long, env)]
    wallet_private_key_path: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct ClusterArgs {
    #[clap(long, env, default_value_t = Cluster::Localnet.url().to_string())]
    rpc: String,

    #[clap(long, env, default_value_t = Cluster::Localnet.ws_url().to_string())]
    ws_rpc: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
enum Commands {
    // Run management commands
    CreateRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCreateRun,
    },
    CloseRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCloseRun,
    },
    UpdateConfig {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandUpdateConfig,
    },
    SetPaused {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandSetPaused,
    },
    SetFutureEpochRates {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandSetFutureEpochRates,
    },
    Checkpoint {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCheckpoint,
    },
    Tick {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTick,
    },
    JsonDumpRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandJsonDumpRun,
    },
    JsonDumpUser {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandJsonDumpUser,
    },
    DownloadResults {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandDownloadResults,
    },
    UploadData {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandUploadData,
    },

    // Authorization commands
    JoinAuthorizationCreate {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandJoinAuthorizationCreate,
    },
    JoinAuthorizationRead {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandJoinAuthorizationRead,
    },
    JoinAuthorizationDelegate {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandJoinAuthorizationDelegate,
    },
    JoinAuthorizationDelete {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandJoinAuthorizationDelete,
    },

    // Treasury commands
    TreasurerClaimRewards {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTreasurerClaimRewards,
    },
    TreasurerTopUpRewards {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTreasurerTopUpRewards,
    },

    // Can join command
    CanJoin {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandCanJoin,
    },

    /// List joinable runs on the coordinator program
    ListRuns {
        /// Path to .env file with RPC and wallet configuration
        #[arg(long)]
        env_file: Option<PathBuf>,
        #[clap(flatten)]
        cluster: ClusterArgs,
        /// Coordinator program ID
        #[arg(long, default_value = "4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7")]
        coordinator_program_id: String,
        /// Only show runs where this pubkey is the join_authority
        #[arg(long)]
        authorizer: Option<String>,
    },

    // Docs generation
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

impl From<ClusterArgs> for Cluster {
    fn from(val: ClusterArgs) -> Self {
        let rpc = val.rpc.trim_matches('"').to_string();
        let ws_rpc = val.ws_rpc.trim_matches('"').to_string();
        Cluster::Custom(rpc, ws_rpc)
    }
}

impl TryInto<Keypair> for WalletArgs {
    type Error = anyhow::Error;

    fn try_into(self) -> std::result::Result<Keypair, Self::Error> {
        let wallet_keypair = match std::env::var("RAW_WALLET_PRIVATE_KEY").ok() {
            Some(raw_wallet_private_key) => {
                if raw_wallet_private_key.starts_with("[") {
                    // assume Keypair::read format
                    match Keypair::read(&mut Cursor::new(raw_wallet_private_key)) {
                        Ok(keypair) => keypair,
                        Err(err) => bail!("{}", err),
                    }
                } else {
                    Keypair::from_base58_string(&raw_wallet_private_key)
                }
            }
            None => match self.wallet_private_key_path {
                Some(wallet_private_key_path) => {
                    match Keypair::read_from_file(wallet_private_key_path) {
                        Ok(wallet_keypair) => wallet_keypair,
                        Err(err) => bail!("{}", err),
                    }
                }
                None => bail!(
                    "No wallet private key! Must pass --wallet-private-key-path or set RAW_WALLET_PRIVATE_KEY"
                ),
            },
        };

        Ok(wallet_keypair)
    }
}

async fn async_main() -> Result<()> {
    let args = CliArgs::parse();

    // If no subcommand is provided, run Docker mode (backward compatibility)
    if args.command.is_none() {
        // Docker mode requires env_file
        let env_file = args.env_file.ok_or_else(|| {
            anyhow::anyhow!("--env-file is required for Docker mode. Use 'run-manager --help' to see available commands.")
        })?;

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

        let authorizer = parse_optional_pubkey(args.authorizer.as_ref(), "authorizer")?;

        let run_mgr = RunManager::new(
            args.coordinator_program_id,
            env_file,
            args.local,
            authorizer,
        )?;
        let result = run_mgr.run(entrypoint).await;
        if let Err(e) = &result {
            error!("Error: {}", e);
            std::process::exit(1);
        }
        return result;
    }

    // Execute blockchain commands
    match args.command.unwrap() {
        Commands::CreateRun {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::CloseRun {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::UpdateConfig {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::SetPaused {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::SetFutureEpochRates {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::Checkpoint {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::Tick {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::JsonDumpRun { cluster, params } => {
            params.execute(create_backend_readonly(cluster)?).await
        }
        Commands::JsonDumpUser { cluster, params } => {
            params.execute(create_backend_readonly(cluster)?).await
        }
        Commands::JoinAuthorizationCreate {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::JoinAuthorizationRead { cluster, params } => {
            params.execute(create_backend_readonly(cluster)?).await
        }
        Commands::JoinAuthorizationDelegate {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::JoinAuthorizationDelete {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::TreasurerClaimRewards {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::TreasurerTopUpRewards {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::CanJoin { cluster, params } => {
            params.execute(create_backend_readonly(cluster)?).await
        }
        Commands::ListRuns {
            env_file,
            cluster,
            coordinator_program_id,
            authorizer,
        } => list_runs(env_file, cluster, coordinator_program_id, authorizer),
        Commands::DownloadResults {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::UploadData {
            cluster,
            wallet,
            params,
        } => params.execute(create_backend(cluster, wallet)?).await,
        Commands::PrintAllHelp { markdown } => {
            assert!(markdown);
            clap_markdown::print_help_markdown::<CliArgs>();
            Ok(())
        }
    }
}

fn list_runs(
    env_file: Option<PathBuf>,
    cluster: ClusterArgs,
    coordinator_program_id: String,
    authorizer: Option<String>,
) -> Result<()> {
    if let Some(env_file) = env_file {
        run_manager::load_and_apply_env_file(&env_file)?;
    }
    let program_id = coordinator_program_id
        .parse::<anchor_client::solana_sdk::pubkey::Pubkey>()
        .context("Failed to parse coordinator program ID")?;
    let rpc = std::env::var("RPC").unwrap_or_else(|_| cluster.rpc.trim_matches('"').to_string());
    let coordinator_client = CoordinatorClient::new(rpc, program_id);
    let runs = coordinator_client.get_all_runs()?;

    if runs.is_empty() {
        println!("No runs found on coordinator program {}", program_id);
        return Ok(());
    }

    let authorizer = parse_optional_pubkey(authorizer.as_ref(), "authorizer")?;
    let delegate_authorizer = parse_delegate_authorizer_from_env()?;
    let wallet_key = run_manager::load_wallet_key()?;
    let user_pubkey = parse_wallet_pubkey(&wallet_key)?;
    let candidates = find_joinable_runs(
        &runs,
        &user_pubkey,
        &coordinator_client,
        authorizer.as_ref(),
        delegate_authorizer.as_ref(),
    )?;

    if candidates.is_empty() {
        println!("No available runs to join");
    } else {
        println!("Found {} joinable run(s):", candidates.len());
        let refs: Vec<_> = candidates.iter().map(|(r, _)| r).collect();
        for line in RunInfo::format_table(&refs) {
            println!("{}", line);
        }
    }

    Ok(())
}

fn create_backend(cluster: ClusterArgs, wallet: WalletArgs) -> Result<SolanaBackend> {
    let wallet_keypair: Keypair = wallet.try_into()?;
    let cluster: Cluster = cluster.into();
    SolanaBackend::new(
        cluster,
        vec![],
        Arc::new(wallet_keypair),
        CommitmentConfig::confirmed(),
    )
}

fn create_backend_readonly(cluster: ClusterArgs) -> Result<SolanaBackend> {
    let cluster: Cluster = cluster.into();
    // For read-only operations, create a dummy keypair
    let dummy_keypair = Keypair::new();
    SolanaBackend::new(
        cluster,
        vec![],
        Arc::new(dummy_keypair),
        CommitmentConfig::confirmed(),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let result = async_main().await;
    if let Err(e) = &result {
        error!("Error: {}", e);
        std::process::exit(1);
    }
    result
}
