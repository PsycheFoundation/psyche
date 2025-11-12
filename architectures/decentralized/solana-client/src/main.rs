use crate::command::can_join::CommandCanJoinParams;
use crate::command::can_join::command_can_join_execute;
use crate::command::checkpoint::CommandCheckpointParams;
use crate::command::checkpoint::command_checkpoint_execute;
use crate::command::close_run::CommandCloseRunParams;
use crate::command::close_run::command_close_run_execute;
use crate::command::create_run::CommandCreateRunParams;
use crate::command::create_run::command_create_run_execute;
use crate::command::json_dump_run::CommandJsonDumpRunParams;
use crate::command::json_dump_run::command_json_dump_run_execute;
use crate::command::json_dump_user::CommandJsonDumpUserParams;
use crate::command::json_dump_user::command_json_dump_user_execute;
use crate::command::set_future_epoch_rates::CommandSetFutureEpochRatesParams;
use crate::command::set_future_epoch_rates::command_set_future_epoch_rates_execute;
use crate::command::set_paused::CommandSetPausedParams;
use crate::command::set_paused::command_set_paused_execute;
use crate::command::tick::CommandTickParams;
use crate::command::tick::command_tick_execute;
use crate::command::treasurer_claim_rewards::CommandTreasurerClaimRewardsParams;
use crate::command::treasurer_claim_rewards::command_treasurer_claim_rewards_execute;
use crate::command::treasurer_top_up_rewards::CommandTreasurerTopUpRewardsParams;
use crate::command::treasurer_top_up_rewards::command_treasurer_top_up_rewards_execute;
use crate::command::update_config::CommandUpdateConfigParams;
use crate::command::update_config::command_update_config_execute;
use crate::{
    app::{AppBuilder, AppParams, TAB_NAMES, Tabs},
    backend::SolanaBackend,
};

use anchor_client::{
    Cluster,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
};
use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use psyche_client::{TrainArgs, print_identity_keys, read_identity_secret_key};
use psyche_core::sha256;
use psyche_network::SecretKey;
use psyche_tui::{
    LogOutput, ServiceInfo,
    logging::{MetricsDestination, OpenTelemetry, RemoteLogsDestination, TraceDestination},
    maybe_start_render_loop,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf, time::Duration};
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::info;

mod app;
mod backend;
mod command;
mod instructions;
mod network_identity;
mod retry;

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

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    ShowStaticP2PIdentity {
        identity_secret_key_path: Option<PathBuf>,
    },
    CreateStaticP2PIdentity {
        save_path: PathBuf,
    },
    CreateRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCreateRunParams,
    },
    CloseRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCloseRunParams,
    },
    SetPaused {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandSetPausedParams,
    },
    UpdateConfig {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandUpdateConfigParams,
    },
    Tick {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTickParams,
    },
    SetFutureEpochRates {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandSetFutureEpochRatesParams,
    },
    TreasurerClaimRewards {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTreasurerClaimRewardsParams,
    },
    TreasurerTopUpRewards {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandTreasurerTopUpRewardsParams,
    },
    Checkpoint {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        wallet: WalletArgs,
        #[clap(flatten)]
        params: CommandCheckpointParams,
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
    CanJoin {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandCanJoinParams,
    },
    JsonDumpRun {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandJsonDumpRunParams,
    },
    JsonDumpUser {
        #[clap(flatten)]
        cluster: ClusterArgs,
        #[clap(flatten)]
        params: CommandJsonDumpUserParams,
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
        Commands::CreateRun {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_create_run_execute(backend, params).await
        }
        Commands::CloseRun {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_close_run_execute(backend, params).await
        }
        Commands::UpdateConfig {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_update_config_execute(backend, params).await
        }
        Commands::SetPaused {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_set_paused_execute(backend, params).await
        }
        Commands::Tick {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_tick_execute(backend, params).await
        }
        Commands::SetFutureEpochRates {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_set_future_epoch_rates_execute(backend, params).await
        }
        Commands::TreasurerClaimRewards {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_treasurer_claim_rewards_execute(backend, params).await
        }
        Commands::TreasurerTopUpRewards {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_treasurer_top_up_rewards_execute(backend, params).await
        }
        Commands::Checkpoint {
            cluster,
            wallet,
            params,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_checkpoint_execute(backend, params).await
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

            let hub_read_token = std::env::var("HF_TOKEN").ok();
            let checkpoint_upload_info = args.checkpoint_config()?;
            let eval_tasks = args.eval_tasks()?;

            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let run_id = args.run_id.trim_matches('"').to_string(); // Trim quotes, if any

            let wallet_keypair: Arc<Keypair> = Arc::new(wallet.try_into()?);

            let solana_pubkey = wallet_keypair.pubkey();
            let wandb_info = args.wandb_info(format!("{run_id}-{solana_pubkey}"))?;

            let identity_secret_key: SecretKey =
                read_identity_secret_key(args.identity_secret_key_path.as_ref())?
                    // Iroh key should be deterministically derived from Solana key
                    .unwrap_or_else(|| {
                        let mut rng =
                            ChaCha8Rng::from_seed(sha256(wallet_keypair.secret().as_bytes()));
                        SecretKey::generate(&mut rng)
                    });

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

            let app = AppBuilder::new(AppParams {
                cancel,
                tx_tui_state,
                identity_secret_key,
                wallet_keypair,
                cluster: cluster.into(),
                backup_clusters,
                run_id,
                p2p_port: args.bind_p2p_port,
                p2p_interface: args.bind_p2p_interface,
                data_parallelism: args.data_parallelism,
                tensor_parallelism: args.tensor_parallelism,
                micro_batch_size: args.micro_batch_size,
                write_gradients_dir: args.write_gradients_dir,
                eval_task_max_docs: args.eval_task_max_docs,
                eval_tasks,
                prompt_task: args.prompt_task,
                checkpoint_upload_info,
                hub_read_token,
                hub_max_concurrent_downloads: args.hub_max_concurrent_downloads,
                wandb_info,
                optim_stats: args.optim_stats_steps,
                grad_accum_in_fp32: args.grad_accum_in_fp32,
                dummy_training_delay_secs: args.dummy_training_delay_secs,
                max_concurrent_parameter_requests: args.max_concurrent_parameter_requests,
                authorizer,
                metrics_local_port: args.metrics_local_port,
                device: args.device,
                sidecar_port: args.sidecar_port,
            })
            .build()
            .await?;

            app.run().await?;
            logger.shutdown()?;

            Ok(())
        }
        Commands::CanJoin { cluster, params } => {
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                Keypair::new().into(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_can_join_execute(backend, params).await
        }
        Commands::JsonDumpRun { cluster, params } => {
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                Keypair::new().into(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_json_dump_run_execute(backend, params).await
        }
        Commands::JsonDumpUser { cluster, params } => {
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                Keypair::new().into(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            command_json_dump_user_execute(backend, params).await
        }
        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);
            let () = clap_markdown::print_help_markdown::<CliArgs>();
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    #[cfg(feature = "python")]
    psyche_python_extension_impl::init_embedded_python();

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
