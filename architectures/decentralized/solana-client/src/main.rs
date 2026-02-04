use crate::app::build_app;
use crate::app::{AppParams, TAB_NAMES, Tabs};

use anchor_client::{
    Cluster,
    solana_sdk::{
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
};
use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use psyche_client::{TrainArgs, print_identity_keys};
use psyche_coordinator::model::{Checkpoint, Model};
use psyche_network::SecretKey;
use psyche_solana_rpc::SolanaBackend;
use psyche_tui::{
    LogOutput, ServiceInfo,
    logging::{MetricsDestination, OpenTelemetry, RemoteLogsDestination, TraceDestination},
    maybe_start_render_loop,
};
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf, time::Duration};
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::info;

mod app;
mod network_identity;

#[derive(Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args, Debug)]
struct WalletArgs {
    #[clap(short, long, env)]
    wallet_private_key_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ClusterArgs {
    #[clap(long, env, default_value_t = Cluster::Localnet.url().to_string())]
    rpc: String,

    #[clap(long, env, default_value_t = Cluster::Localnet.ws_url().to_string())]
    ws_rpc: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
enum Commands {
    ShowStaticP2PIdentity {
        identity_secret_key_path: Option<PathBuf>,
    },
    CreateStaticP2PIdentity {
        save_path: PathBuf,
    },
    Train {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(flatten)]
        args: TrainArgs,

        #[clap(long, env, default_value_t = String::from(""))]
        rpc_2: String,
        #[clap(long, env, default_value_t = String::from(""))]
        ws_rpc_2: String,
        #[clap(long, env, default_value_t = String::from(""))]
        rpc_3: String,
        #[clap(long, env, default_value_t = String::from(""))]
        ws_rpc_3: String,
        #[clap(long, env)]
        authorizer: Option<Pubkey>,
    },
    Predownload {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env, action)]
        model: bool,

        #[clap(long, env)]
        eval_tasks: Option<String>,

        #[clap(long, env, default_value_t = 3)]
        hub_max_concurrent_downloads: usize,
    },
    // Prints the help, optionally as markdown. Used for docs generation.
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

    match args.command {
        Commands::ShowStaticP2PIdentity {
            identity_secret_key_path,
        } => print_identity_keys(identity_secret_key_path.as_ref()),
        Commands::CreateStaticP2PIdentity { save_path } => {
            let identity_secret_key = SecretKey::generate(&mut rand::rng());
            std::fs::write(&save_path, identity_secret_key.to_bytes())?;
            print_identity_keys(Some(&save_path))?;
            println!("Wrote secret key to {}", save_path.display());
            Ok(())
        }
        Commands::Train {
            cluster,
            wallet,
            args,
            rpc_2,
            ws_rpc_2,
            rpc_3,
            ws_rpc_3,
            authorizer,
        } => {
            psyche_client::prepare_environment();
            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let wallet_keypair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            info!("Solana wallet pubkey: {}", wallet_keypair.pubkey());

            let logger = psyche_tui::logging()
                .with_output(args.logs)
                .with_log_file(args.write_log.clone())
                .with_metrics_destination(args.oltp_metrics_url.clone().map(|endpoint| {
                    MetricsDestination::OpenTelemetry(OpenTelemetry {
                        endpoint,
                        authorization_header: args.oltp_auth_header.clone(),
                        report_interval: args.oltp_report_interval,
                    })
                }))
                .with_trace_destination(args.oltp_tracing_url.clone().map(|endpoint| {
                    TraceDestination::OpenTelemetry(OpenTelemetry {
                        endpoint,
                        authorization_header: args.oltp_auth_header.clone(),
                        report_interval: args.oltp_report_interval,
                    })
                }))
                .with_remote_logs(args.oltp_logs_url.clone().map(|endpoint| {
                    RemoteLogsDestination::OpenTelemetry(OpenTelemetry {
                        endpoint,
                        authorization_header: args.oltp_auth_header.clone(),
                        report_interval: Duration::from_secs(4),
                    })
                }))
                .with_service_info(ServiceInfo {
                    name: "psyche-solana-client".to_string(),
                    instance_id: wallet_keypair.pubkey().to_string(),
                    namespace: "psyche".to_string(),
                    deployment_environment: std::env::var("DEPLOYMENT_ENV")
                        .unwrap_or("development".to_string()),
                    run_id: Some(args.run_id.clone()),
                })
                .init()?;

            let (cancel, tx_tui_state) = maybe_start_render_loop(
                (args.logs == LogOutput::TUI).then(|| Tabs::new(Default::default(), &TAB_NAMES)),
            )?;

            let mut backup_clusters = Vec::new();
            for (rpc, ws) in [(rpc_2, ws_rpc_2), (rpc_3, ws_rpc_3)] {
                let rpc = if rpc.is_empty() {
                    cluster.rpc.clone()
                } else {
                    rpc
                };
                let ws = if ws.is_empty() {
                    cluster.ws_rpc.clone()
                } else {
                    ws
                };
                backup_clusters.push(Cluster::Custom(rpc, ws))
            }

            let app = build_app(AppParams {
                cancel,
                tx_tui_state,
                wallet_keypair,
                cluster: cluster.into(),
                backup_clusters,
                authorizer,
                train_args: args,
            })
            .await?;

            app.run().await?;
            logger.shutdown()?;

            Ok(())
        }
        Commands::Predownload {
            cluster,
            run_id,
            model,
            eval_tasks,
            hub_max_concurrent_downloads,
        } => {
            use anchor_client::solana_sdk::commitment_config::CommitmentConfig;

            // Create a read-only backend (no wallet needed)
            let dummy_keypair = Keypair::new();
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                Arc::new(dummy_keypair),
                CommitmentConfig::confirmed(),
            )?;

            let coordinator_instance =
                psyche_solana_coordinator::find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;

            let coordinator_account_state = backend
                .get_coordinator_account(&coordinator_instance_state.coordinator_account)
                .await?
                .state
                .coordinator;

            if model {
                #[allow(irrefutable_let_patterns)]
                let Model::LLM(model_config) = coordinator_account_state.model else {
                    bail!("Model is not an LLM, unsure how to predownload.");
                };

                match model_config.checkpoint {
                    Checkpoint::Ephemeral => {
                        bail!("Can't predownload model with ephemeral checkpoint.")
                    }
                    Checkpoint::Dummy(hub_repo)
                    | Checkpoint::Hub(hub_repo)
                    | Checkpoint::P2P(hub_repo) => {
                        let repo_id = hub_repo.repo_id.to_string();
                        let revision = hub_repo.revision.map(|s| s.to_string());
                        println!(
                            "Predownloading model {repo_id} revision {}",
                            revision.as_ref().unwrap_or(&"main".to_string())
                        );

                        let hub_read_token = std::env::var("HF_TOKEN").ok();
                        let cache_folder = None; // Uses HF_HOME env var

                        psyche_data_provider::download_model_repo_async(
                            &repo_id,
                            revision,
                            cache_folder,
                            hub_read_token,
                            Some(hub_max_concurrent_downloads),
                            true,
                        )
                        .await?;
                    }
                    Checkpoint::Gcs(gcs_repo) | Checkpoint::P2PGcs(gcs_repo) => {
                        let bucket = gcs_repo.bucket.to_string();
                        let prefix: Option<String> = gcs_repo.prefix.map(|p| p.to_string());
                        println!(
                            "Predownloading model from gs://{}/{}",
                            bucket,
                            prefix.as_deref().unwrap_or("")
                        );

                        psyche_data_provider::download_model_from_gcs_async(
                            &bucket,
                            prefix.as_deref(),
                        )
                        .await?;
                    }
                };

                println!("Model predownloaded successfully.");
            }

            if let Some(eval_tasks) = eval_tasks {
                let _ = TrainArgs::eval_tasks_from_args(&eval_tasks, 0)?;
                println!("Eval tasks `{eval_tasks}` predownloaded successfully.");
            }

            Ok(())
        }
        Commands::PrintAllHelp { markdown } => {
            assert!(markdown);
            clap_markdown::print_help_markdown::<CliArgs>();
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    #[cfg(feature = "python")]
    psyche_python_extension_impl::init_embedded_python()?;

    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .thread_stack_size(10 * 1024 * 1024)
        .build()
        .unwrap();
    let ret = runtime.block_on(async_main());
    runtime.shutdown_timeout(Duration::from_millis(1000));
    ret
}
