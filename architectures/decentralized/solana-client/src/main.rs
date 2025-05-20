use crate::{
    app::{AppBuilder, AppParams, Tabs, TAB_NAMES},
    backend::SolanaBackend,
};

use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        native_token::lamports_to_sol,
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
    Cluster,
};
use anyhow::{bail, Context, Result};
use bytemuck::Zeroable;
use clap::{Args, Parser, Subcommand};
use psyche_client::{print_identity_keys, read_identity_secret_key, TrainArgs};
use psyche_coordinator::{
    get_data_index_for_step,
    model::{Checkpoint, Model},
    CoordinatorConfig, CoordinatorProgress,
};
use psyche_core::sha256;
use psyche_network::SecretKey;
use psyche_solana_coordinator::find_coordinator_instance;
use psyche_tui::{maybe_start_render_loop, LogOutput};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf, time::Duration};
use time::OffsetDateTime;
use tokio::{
    runtime::Builder,
    time::{interval, MissedTickBehavior},
};
use tracing::{info, Level};

mod app;
mod backend;
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
            },
            None => match self.wallet_private_key_path {
                Some(wallet_private_key_path) => match Keypair::read_from_file(wallet_private_key_path) {
                    Ok(wallet_keypair) => wallet_keypair,
                    Err(err) => bail!("{}", err),
                },
                None => bail!("No wallet private key! Must pass --wallet-private-key-path or set RAW_WALLET_PRIVATE_KEY")
            }
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
            let created = backend
                .create_run(
                    run_id.clone(),
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
            println!("Closed run {} with transaction {}", run_id, closed);
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
                    coordinator_instance,
                    coordinator_account,
                    metadata,
                    config,
                    model,
                    progress,
                )
                .await?;
            println!("Updated config of {} with transaction {}", run_id, set);
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
                .set_paused(coordinator_instance, coordinator_account, paused)
                .await?;
            println!(
                "Set pause state to {} on run {} with transaction {}",
                paused, run_id, set
            );
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
                println!("Ticked run {} with transaction {}", run_id, ticked);
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
                    coordinator_instance,
                    coordinator_account,
                    earning_rate,
                    slashing_rate,
                )
                .await?;
            println!(
                "Set earning rate to {:?} and slashing rate to {:?} on run {} with transaction {}",
                earning_rate, slashing_rate, run_id, set
            );
            println!("\n===== Logs =====");
            for log in backend.get_logs(&set).await? {
                println!("{log}");
            }
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
            let wandb_info = args.wandb_info(format!("{}-{}", run_id, solana_pubkey))?;

            let identity_secret_key: SecretKey =
                read_identity_secret_key(args.identity_secret_key_path.as_ref())?
                    // Iroh key should be deterministically derived from Solana key
                    .unwrap_or_else(|| {
                        let mut rng =
                            ChaCha8Rng::from_seed(sha256(wallet_keypair.secret().as_bytes()));
                        SecretKey::generate(&mut rng)
                    });

            let logger = psyche_tui::init_logging(
                args.logs,
                Level::INFO,
                args.write_log.clone(),
                true,
                Some(identity_secret_key.public().fmt_short()),
            )?;

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

            let (mut app, allowlist, p2p, state_options) = AppBuilder::new(AppParams {
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
                wandb_info,
                optim_stats: args.optim_stats_steps,
                grad_accum_in_fp32: args.grad_accum_in_fp32,
                dummy_training_delay_secs: args.dummy_training_delay_secs,
                max_concurrent_parameter_requests: args.max_concurrent_parameter_requests,
                max_concurrent_downloads: args.max_concurrent_downloads,
                authorizer,
            })
            .build()
            .await
            .unwrap();

            app.run(allowlist, p2p, state_options).await?;
            logger.shutdown()?;

            Ok(())
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
    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .thread_stack_size(10 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(async_main())
}
