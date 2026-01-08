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
use psyche_network::SecretKey;
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
        Commands::PrintAllHelp { markdown } => {
            assert!(markdown);
            clap_markdown::print_help_markdown::<CliArgs>();
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    // Multi-threaded runtime for ML workloads
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}
