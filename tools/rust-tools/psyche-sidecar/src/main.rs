use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use futures::future::try_join_all;
use std::process::{Command, Stdio};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "psyche-sidecar")]
#[command(about = "Multi-node sidecar for Psyche distributed training")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Python {
        /// Address of the main node
        #[arg(long, env = "PSYCHE_MAIN_HOST")]
        main_host: String,

        /// Port for coordination
        #[arg(long, default_value = "34567")]
        port: u16,

        /// World size for distributed training
        #[arg(long, env = "PSYCHE_WORLD_SIZE")]
        world_size: usize,

        /// Start rank for distributed training
        #[arg(long, env = "PSYCHE_START_RANK")]
        start_rank: usize,

        #[arg(long)]
        start_device: Option<usize>,

        #[arg(long)]
        num_local_ranks: Option<usize>,

        /// Backend for torch.distributed (default: nccl)
        #[arg(long, default_value = "nccl")]
        backend: String,
    },

    /// Run Rust sidecar process (TODO: implement)
    Rust,

    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Commands::Python {
            main_host,
            port,
            world_size,
            start_rank,
            start_device,
            num_local_ranks,
            backend,
        } => {
            if !tch::Cuda::is_available() {
                bail!("CUDA not avaiable");
            }

            let num_local_ranks =
                num_local_ranks.unwrap_or_else(|| tch::Cuda::device_count() as usize);
            let last_rank = start_rank + (num_local_ranks);
            if last_rank >= world_size {
                bail!("World size is too small: world_size <= start_rank + num_local_ranks ");
            }
            let last_rank = std::cmp::min(last_rank, world_size);

            info!(
                "Starting Python sidecars for ranks {} to {}",
                start_rank,
                world_size - 1
            );

            // Spawn all tasks
            let mut sidecar_tasks = Vec::new();
            for rank in start_rank..last_rank {
                let main_host = main_host.clone();
                let backend = backend.clone();
                let parent_pid = std::process::id();

                sidecar_tasks.push(tokio::spawn(async move {
                    info!("Starting Python sidecars for rank {}", rank);
                    let start_device = start_device.unwrap_or_default();
                    let device = rank - start_rank + start_device;

                    run_python_sidecar(
                        main_host, port, world_size, rank, device, backend, parent_pid,
                    )
                    .await
                }));
            }

            match try_join_all(sidecar_tasks).await {
                Ok(_) => info!("Sidecar processes completed successfully"),
                Err(e) => bail!("One or more sidecar processes failed with error: {e}"),
            }

            Ok(())
        }
        Commands::Rust => {
            unimplemented!("Rust sidecar not yet implemented");
        }
        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<Args>();

            return Ok(());
        }
    }
}

async fn run_python_sidecar(
    main_host: String,
    port: u16,
    world_size: usize,
    rank: usize,
    device: usize,
    backend: String,
    parent_pid: u32,
) -> Result<()> {
    let init_method = format!("tcp://{main_host}:{port}");

    info!(
        "Connecting to master at {} (rank {} to {})",
        init_method, rank, world_size
    );

    let mut cmd = Command::new("python");
    cmd.arg("-m")
        .arg("psyche.sidecar")
        .arg("--backend")
        .arg(&backend)
        .arg("--init-method")
        .arg(&init_method)
        .arg("--world-size")
        .arg(world_size.to_string())
        .arg("--rank")
        .arg(rank.to_string())
        .arg("--device")
        .arg(device.to_string())
        .arg("--parent-pid")
        .arg(parent_pid.to_string());

    // forward IO for logging
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    info!("Executing: {cmd:?}",);

    let mut child = cmd.spawn()?;
    let exit_status = child.wait()?;

    if exit_status.success() {
        info!("Python sidecar completed successfully");
        Ok(())
    } else {
        error!(
            "Python sidecar failed with exit code: {:?}",
            exit_status.code()
        );
        bail!("Python sidecar process failed")
    }
}
