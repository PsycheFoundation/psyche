use crate::app::{TAB_NAMES, Tabs, build_app};

use anyhow::Result;
use clap::{Parser, Subcommand};
use psyche_client::{TrainArgs, print_identity_keys, read_identity_secret_key};
use psyche_network::SecretKey;
use psyche_tui::{
    LogOutput, ServiceInfo,
    logging::{MetricsDestination, OpenTelemetry, RemoteLogsDestination, TraceDestination},
    maybe_start_render_loop,
};
use std::{path::PathBuf, time::Duration};
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::info;

mod app;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Displays the client's unique identifier, used to participate in training runs.
    ShowIdentity {
        /// Path to the clients secret key. Create a new random one running `openssl rand 32 > secret.key` or use the `RAW_IDENTITY_SECRET_KEY` environment variable.
        #[clap(long)]
        identity_secret_key_path: Option<PathBuf>,
    },
    /// Allows the client to join a training run and contribute to the model's training process.
    Train {
        #[clap(flatten)]
        args: TrainArgs,

        #[clap(long, env)]
        server_addr: String,
    },
    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

async fn async_main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::ShowIdentity {
            identity_secret_key_path,
        } => print_identity_keys(identity_secret_key_path.as_ref()),
        Commands::Train { args, server_addr } => {
            psyche_client::prepare_environment();

            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let identity_secret_key =
                read_identity_secret_key(args.identity_secret_key_path.as_ref())?
                    .unwrap_or_else(|| SecretKey::generate(&mut rand::rng()));

            let logger = psyche_tui::logging()
                .with_output(args.logs)
                .with_log_file(args.write_log.clone())
                .with_metrics_destination(
                    args.otlp_metrics_url
                        .clone()
                        .map_or_else(|| args.oltp_metrics_url.clone(), Some)
                        .map(|endpoint| {
                            MetricsDestination::OpenTelemetry(OpenTelemetry {
                                endpoint,
                                authorization_header: args
                                    .otlp_auth_header
                                    .clone()
                                    .map_or_else(|| args.oltp_auth_header.clone(), Some),
                                report_interval: args.otlp_report_interval,
                            })
                        }),
                )
                .with_trace_destination(
                    args.otlp_tracing_url
                        .clone()
                        .map_or_else(|| args.oltp_tracing_url.clone(), Some)
                        .map(|endpoint| {
                            TraceDestination::OpenTelemetry(OpenTelemetry {
                                endpoint,
                                authorization_header: args
                                    .otlp_auth_header
                                    .clone()
                                    .map_or_else(|| args.oltp_auth_header.clone(), Some),
                                report_interval: args.otlp_report_interval,
                            })
                        }),
                )
                .with_remote_logs(
                    args.otlp_logs_url
                        .clone()
                        .map_or_else(|| args.oltp_logs_url.clone(), Some)
                        .map(|endpoint| {
                            RemoteLogsDestination::OpenTelemetry(OpenTelemetry {
                                endpoint,
                                authorization_header: args
                                    .otlp_auth_header
                                    .clone()
                                    .map_or_else(|| args.oltp_auth_header.clone(), Some),
                                report_interval: Duration::from_secs(4),
                            })
                        }),
                )
                .with_service_info(ServiceInfo {
                    name: "psyche-centralized-client".to_string(),
                    instance_id: identity_secret_key.public().to_string(),
                    namespace: "psyche".to_string(),
                    deployment_environment: std::env::var("DEPLOYMENT_ENV")
                        .unwrap_or("development".to_string()),
                    run_id: Some(args.run_id.clone()),
                })
                .init()?;

            let (cancel, tx_tui_state) = maybe_start_render_loop(
                (args.logs == LogOutput::TUI).then(|| Tabs::new(Default::default(), &TAB_NAMES)),
            )?;

            let (mut app, allowlist, p2p, state_options) =
                build_app(cancel, server_addr, tx_tui_state, args)
                    .await
                    .unwrap();

            app.run(allowlist, p2p, state_options).await?;
            logger.shutdown()?;

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

fn main() -> Result<()> {
    #[cfg(feature = "python")]
    psyche_python_extension_impl::init_embedded_python()?;

    // let shutdown_handler =
    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .thread_stack_size(10 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(async_main())?;
    // shutdown_handler.shutdown()?;
    Ok(())
}
