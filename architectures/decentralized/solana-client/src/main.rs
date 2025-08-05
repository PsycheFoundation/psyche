use crate::{
    app::{AppBuilder, AppParams, TAB_NAMES, Tabs},
    backend::SolanaBackend,
};

use anchor_client::{
    Client, Cluster,
    anchor_lang::system_program,
    solana_sdk::{
        commitment_config::{CommitmentConfig, CommitmentLevel},
        native_token::lamports_to_sol,
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
};
use anyhow::{Context, Result, bail};
use bytemuck::Zeroable;
use clap::{Args, Parser, Subcommand};
use psyche_client::{TrainArgs, print_identity_keys, read_identity_secret_key};
use psyche_coordinator::{
    CoordinatorConfig, CoordinatorProgress, RunState, get_data_index_for_step,
    model::{Checkpoint, HubRepo, Model},
};
use psyche_core::{FixedString, sha256};
use psyche_network::SecretKey;
use psyche_solana_authorizer::state::Authorization;
use psyche_solana_coordinator::{find_coordinator_instance, logic::JOIN_RUN_AUTHORIZATION_SCOPE};
use psyche_tui::{
    LogOutput, ServiceInfo,
    logging::{MetricsDestination, OpenTelemetry, RemoteLogsDestination, TraceDestination},
    maybe_start_render_loop,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf, time::Duration};
use time::OffsetDateTime;
use tokio::{
    runtime::Builder,
    time::{MissedTickBehavior, interval},
};
use tracing::info;

mod app;
mod backend;
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

#[derive(Serialize, Deserialize, Zeroable)]
struct State {
    pub config: CoordinatorConfig,
    pub model: Model,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum ShowChoices {
    Config,
    Model,
    EpochState,
    Progress,
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

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        treasurer_index: Option<u64>,

        #[clap(long, env)]
        treasurer_collateral_mint: Option<String>,

        #[clap(long)]
        join_authority: Option<String>,
    },
    CloseRun {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,
    },
    SetPaused {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(long, env)]
        run_id: String,

        #[clap(long, env)]
        treasurer_index: Option<u64>,

        #[clap(short, long, env)]
        resume: bool,
    },
    UpdateConfig {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        treasurer_index: Option<u64>,

        #[clap(long, env)]
        config_path: Option<PathBuf>,

        #[clap(long, env)]
        restart_from_step: Option<u32>,

        #[clap(long, env)]
        switch_to_hub: bool,

        // metadata
        #[clap(long)]
        name: Option<String>,

        #[clap(long)]
        description: Option<String>,

        #[clap(long)]
        num_parameters: Option<u64>,

        #[clap(long)]
        vocab_size: Option<u64>,
        // end metadata
    },
    Tick {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env, default_value_t = 1000)]
        ms_interval: u64,

        #[clap(long, env)]
        count: Option<u64>,
    },
    SetFutureEpochRates {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        treasurer_index: Option<u64>,

        #[clap(long, env)]
        earning_rate: Option<u64>,

        #[clap(long, env)]
        slashing_rate: Option<u64>,
    },
    Show {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(short, long, env)]
        run_id: String,

        choice: ShowChoices,
    },
    Checkpoint {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        repo: String,

        #[clap(long, env)]
        revision: Option<String>,
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
        wallet: WalletArgs,

        #[clap(short, long)]
        pubkey: Option<String>,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        authorizer: Option<Pubkey>,

        #[clap(long, env, action)]
        predownload_model: bool,

        #[clap(long, env, action)]
        predownload_eval_tasks: Option<String>,

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
            let identity_secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(&save_path, identity_secret_key.secret().as_bytes())?;
            print_identity_keys(Some(&save_path))?;
            println!("Wrote secret key to {}", save_path.display());
            Ok(())
        }
        Commands::CreateRun {
            cluster,
            wallet,
            run_id,
            treasurer_index,
            treasurer_collateral_mint,
            join_authority,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();

            if treasurer_index.is_some() && treasurer_collateral_mint.is_none() {
                bail!(
                    "treasurer_index is set, but treasurer_collateral_mint is not. Please provide a collateral mint address."
                );
            }
            let treasurer_index_and_collateral_mint =
                treasurer_collateral_mint.map(|treasurer_collateral_mint| {
                    (
                        SolanaBackend::compute_deterministic_treasurer_index(
                            &run_id,
                            treasurer_index,
                        ),
                        Pubkey::from_str(&treasurer_collateral_mint).unwrap(),
                    )
                });

            let created = backend
                .create_run(
                    &run_id,
                    treasurer_index_and_collateral_mint,
                    join_authority.map(|address| Pubkey::from_str(&address).unwrap()),
                )
                .await?;
            let locked = backend.get_balance(&created.account).await?;
            println!(
                "Created run {} with transactions signatures: {:?}",
                run_id, created.create_signatures,
            );
            println!("Instance account: {}", created.instance);
            println!("Coordinator account: {}", created.account);
            println!("Locked for storage: {:.9} SOL", lamports_to_sol(locked));
            Ok(())
        }
        Commands::CloseRun {
            cluster,
            wallet,
            run_id,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let balance = backend.get_balance(&key_pair.pubkey()).await?;
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let closed = backend
                .close_run(coordinator_instance, coordinator_account)
                .await?;
            println!("Closed run {run_id} with transaction {closed}");
            let recovered = backend.get_balance(&key_pair.pubkey()).await? - balance;
            println!("Recovered {:.9} SOL", lamports_to_sol(recovered));
            println!("\n===== Logs =====");
            for log in backend.get_logs(&closed).await? {
                println!("{log}");
            }
            Ok(())
        }
        Commands::UpdateConfig {
            cluster,
            wallet,
            run_id,
            treasurer_index,
            config_path,
            restart_from_step,
            switch_to_hub,
            name,
            description,
            num_parameters,
            vocab_size,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let account = backend
                .get_coordinator_account(&coordinator_account)
                .await?;
            let progress = restart_from_step.map(|step| CoordinatorProgress {
                epoch: account.state.coordinator.progress.epoch,
                step,
                epoch_start_data_index: get_data_index_for_step(&account.state.coordinator, step),
            });

            let (config, mut model) = match config_path {
                Some(config_path) => {
                    let state: State = toml::from_str(std::str::from_utf8(
                        &std::fs::read(&config_path).with_context(|| {
                            format!("failed to read config toml file {config_path:?}")
                        })?,
                    )?)
                    .with_context(|| format!("failed to parse config toml file {config_path:?}"))?;

                    (Some(state.config), Some(state.model))
                }
                None => (None, None),
            };

            model = if switch_to_hub {
                let Model::LLM(mut llm) = model.unwrap_or(account.state.coordinator.model);
                match llm.checkpoint {
                    Checkpoint::P2P(hub_repo) | Checkpoint::Dummy(hub_repo) => {
                        llm.checkpoint = Checkpoint::Hub(hub_repo)
                    }
                    _ => {}
                }
                Some(Model::LLM(llm))
            } else {
                model
            };

            let metadata = {
                let mut metadata = account.state.metadata;

                if let Some(name) = name {
                    metadata.name = name
                        .as_str()
                        .try_into()
                        .context("run metadata: name failed to convert to FixedString")?;
                }
                if let Some(description) = description {
                    metadata.description = description
                        .as_str()
                        .try_into()
                        .context("run metadata: description failed to convert to FixedString")?;
                }
                if let Some(num_parameters) = num_parameters {
                    metadata.num_parameters = num_parameters;
                }
                if let Some(vocab_size) = vocab_size {
                    metadata.vocab_size = vocab_size;
                }
                // only include if it's different
                (metadata != account.state.metadata).then_some(metadata)
            };

            if metadata.is_none() && config.is_none() && model.is_none() && progress.is_none() {
                bail!("this invocation would not update anything, bailing.")
            }

            let set: anchor_client::solana_sdk::signature::Signature = backend
                .update(
                    &run_id,
                    treasurer_index,
                    &coordinator_account,
                    metadata,
                    config,
                    model,
                    progress,
                )
                .await?;
            println!("Updated config of {run_id} with transaction {set}");
            println!("\n===== Logs =====");
            for log in backend.get_logs(&set).await? {
                println!("{log}");
            }
            Ok(())
        }
        Commands::SetPaused {
            cluster,
            wallet,
            run_id,
            treasurer_index,
            resume,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let paused = !resume;
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let set = backend
                .set_paused(&run_id, treasurer_index, &coordinator_account, paused)
                .await?;
            println!("Set pause state to {paused} on run {run_id} with transaction {set}");
            println!("\n===== Logs =====");
            for log in backend.get_logs(&set).await? {
                println!("{log}");
            }
            Ok(())
        }
        Commands::Tick {
            cluster,
            wallet,
            run_id,
            ms_interval,
            count,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let mut interval = interval(Duration::from_millis(ms_interval));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            for _ in 0..count.unwrap_or(u64::MAX) {
                let ticked = backend
                    .tick(coordinator_instance, coordinator_account)
                    .await?;
                println!("Ticked run {run_id} with transaction {ticked}");
                println!("\n===== Logs =====");
                for log in backend.get_logs(&ticked).await? {
                    println!("{log}");
                }
                println!();
                interval.tick().await;
            }

            Ok(())
        }
        Commands::SetFutureEpochRates {
            cluster,
            wallet,
            run_id,
            treasurer_index,
            earning_rate,
            slashing_rate,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let set = backend
                .set_future_epoch_rates(
                    &run_id,
                    treasurer_index,
                    &coordinator_account,
                    earning_rate,
                    slashing_rate,
                )
                .await?;
            println!(
                "Set earning rate to {earning_rate:?} and slashing rate to {slashing_rate:?} on run {run_id} with transaction {set}"
            );
            println!("\n===== Logs =====");
            for log in backend.get_logs(&set).await? {
                println!("{log}");
            }
            Ok(())
        }
        Commands::Checkpoint {
            cluster,
            wallet,
            run_id,
            repo,
            revision,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let checkpoint = backend
                .checkpoint(
                    coordinator_instance,
                    coordinator_account,
                    HubRepo {
                        repo_id: FixedString::from_str_truncated(&repo),
                        revision: revision
                            .clone()
                            .map(|x| FixedString::from_str_truncated(&x)),
                    },
                )
                .await?;
            println!(
                "Checkpointed to repo {}{}on run {} with transaction {}",
                repo,
                match revision {
                    Some(revision) => format!(" ({revision}) "),
                    None => " ".to_string(),
                },
                run_id,
                checkpoint
            );
            Ok(())
        }
        Commands::Show {
            cluster,
            run_id,
            choice,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(Keypair::new());
            let backend = SolanaBackend::new(
                cluster.into(),
                vec![],
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let coordinator_instance = find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await?;
            let coordinator_account = coordinator_instance_state.coordinator_account;
            let account = backend
                .get_coordinator_account(&coordinator_account)
                .await?;
            match choice {
                ShowChoices::Config => println!(
                    "{}",
                    toml::to_string_pretty(&account.state.coordinator.config)?
                ),
                ShowChoices::Model => println!(
                    "{}",
                    toml::to_string_pretty(&account.state.coordinator.model)?
                ),
                ShowChoices::EpochState => println!(
                    "{}",
                    toml::to_string_pretty(&account.state.coordinator.epoch_state)?
                ),
                ShowChoices::Progress => println!(
                    "{}",
                    toml::to_string_pretty(&account.state.coordinator.progress)?
                ),
            }
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
                    instance_id: identity_secret_key.public().to_string(),
                    namespace: "psyche".to_string(),
                    deployment_environment: std::env::var("DEPLOYMENT_ENV")
                        .unwrap_or("development".to_string()),
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
                checkpoint_upload_info,
                hub_read_token,
                hub_max_concurrent_downloads: args.hub_max_concurrent_downloads,
                wandb_info,
                optim_stats: args.optim_stats_steps,
                grad_accum_in_fp32: args.grad_accum_in_fp32,
                dummy_training_delay_secs: args.dummy_training_delay_secs,
                max_concurrent_parameter_requests: args.max_concurrent_parameter_requests,
                max_concurrent_downloads: args.max_concurrent_downloads,
                authorizer,
                metrics_local_port: args.metrics_local_port,
            })
            .build()
            .await
            .unwrap();

            app.run().await?;
            logger.shutdown()?;

            Ok(())
        }

        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<CliArgs>();

            Ok(())
        }
        Commands::CanJoin {
            cluster,
            wallet,
            run_id,
            authorizer,
            pubkey,
            predownload_model,
            predownload_eval_tasks,
            hub_max_concurrent_downloads,
        } => {
            // when we call join_run, we check
            //  constraint = authorization.is_valid_for(
            //     &coordinator_instance.join_authority,
            //     user.key,
            //     JOIN_RUN_AUTHORIZATION_SCOPE,
            // )
            // so replicate that check here.
            let cluster: Cluster = cluster.into();
            let fake_payer = Arc::new(Keypair::new());
            let commitment = CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            };
            let client = Client::new_with_options(cluster.clone(), fake_payer.clone(), commitment);
            let backend = SolanaBackend::new(cluster.clone(), vec![], fake_payer, commitment)
                .context("Failed to create backend on cluster")?;

            let coordinator_instance =
                psyche_solana_coordinator::find_coordinator_instance(&run_id);
            let coordinator_instance_state = backend
                .get_coordinator_instance(&coordinator_instance)
                .await
                .context("failed to get coordinator instance")?;

            let authorization = psyche_solana_authorizer::find_authorization(
                &coordinator_instance_state.join_authority,
                &authorizer.unwrap_or(system_program::ID),
                psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
            );

            let authorization_state: Authorization = client
                .program(psyche_solana_authorizer::ID)?
                .account(authorization)
                .await
                .with_context(|| {
                    format!(
                        "failed to get authorization at addr {authorization} on cluster {cluster}"
                    )
                })?;

            let maybe_wallet: Result<Keypair, _> = wallet.try_into();
            let solana_pubkey: Pubkey = match (maybe_wallet, pubkey) {
                (Ok(_), Some(_)) => bail!("passed both private key and pubkey args. pick one."),
                (Ok(wallet), None) => wallet.pubkey(),
                (Err(_), Some(pk)) => Pubkey::from_str(&pk)?,
                (Err(e), None) => return Err(e),
            };
            if !authorization_state.is_valid_for(
                &coordinator_instance_state.join_authority,
                &solana_pubkey,
                JOIN_RUN_AUTHORIZATION_SCOPE,
            ) {
                bail!("Authorization invalid for run id {run_id} using pubkey {solana_pubkey}");
            }
            println!("authorization valid for run id {run_id} using pubkey {solana_pubkey}");

            let coordinator_account_state = backend
                .get_coordinator_account(&coordinator_instance_state.coordinator_account)
                .await?
                .state
                .coordinator;

            let is_paused = matches!(coordinator_account_state.run_state, RunState::Paused);

            if !is_paused {
                let client_with_our_key = coordinator_account_state
                    .epoch_state
                    .clients
                    .iter()
                    .find(|c| c.id.signer == solana_pubkey);
                if client_with_our_key.is_some() {
                    bail!(
                        "A client with our pubkey {solana_pubkey} is in the current epoch, you can't join with this key!"
                    );
                }
            }
            if predownload_model {
                // it would also be reasonable to download the model if we're in WaitingForClients and the checkpoint is not P2P,
                // but that could cause you to miss the transition to Warmup, so we won't do that for now.
                if !is_paused {
                    println!("run is in progress, skipping model predownload.");
                    return Ok(());
                }

                #[allow(irrefutable_let_patterns)]
                let Model::LLM(model) = coordinator_account_state.model else {
                    bail!("model is not an LLM, unsure how to predownload.");
                };

                let checkpoint = match model.checkpoint {
                    Checkpoint::Ephemeral => {
                        bail!("Can't predownload model with ephemeral checkpoint.")
                    }
                    Checkpoint::Dummy(hub_repo)
                    | Checkpoint::Hub(hub_repo)
                    | Checkpoint::P2P(hub_repo) => hub_repo,
                };
                let repo_id = checkpoint.repo_id.to_string();
                let revision = checkpoint.revision.map(|s| s.to_string());
                println!(
                    "Predownloading model {repo_id} revision {}",
                    revision.as_ref().unwrap_or(&"main".to_string())
                );
                let hub_read_token = std::env::var("HF_TOKEN").ok();

                // If you pass None as a cache folder, it'll use the env var `HF_HOME`.
                let cache_folder = None;

                psyche_data_provider::download_model_repo_async(
                    &repo_id,
                    revision,
                    cache_folder,
                    hub_read_token,
                    Some(hub_max_concurrent_downloads),
                    true,
                )
                .await?;
                println!("Model predownloaded successfully.")
            }
            if let Some(predownload_eval_tasks) = predownload_eval_tasks {
                let _ = TrainArgs::eval_tasks_from_args(&predownload_eval_tasks, 0, 0)?;
                println!("Eval tasks `{predownload_eval_tasks}` predownloaded successfully.");
            }
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
