use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
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

        // /// Rank of this process
        // #[arg(long, env = "PSYCHE_RANK")]
        // rank: usize,
        /// Start rank for distributed training
        #[arg(long, env = "PSYCHE_START_RANK")]
        start_rank: usize,

        /// Backend for torch.distributed (default: nccl)
        #[arg(long, default_value = "nccl")]
        backend: String,

        /// Parent process ID for monitoring
        #[arg(long, env = "PSYCHE_PARENT_PID")]
        parent_pid: Option<u32>,
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
            backend,
            parent_pid,
        } => {
            info!(
                "Starting Python sidecars for ranks {} to {}",
                start_rank,
                world_size - 1
            );

            // Spawn all tasks
            let mut tasks = Vec::new();
            for rank in start_rank..world_size {
                let main_host = main_host.clone();
                let backend = backend.clone();

                tasks.push(tokio::spawn(async move {
                    info!("Starting Python sidecars for rank {}", rank);
                    let device = rank - start_rank;
                    run_python_sidecar(
                        main_host, port, world_size, rank, device, backend, parent_pid,
                    )
                    .await
                }));
            }

            // Wait for all tasks
            let mut all_success = true;
            for (i, task) in tasks.into_iter().enumerate() {
                let rank = start_rank + i;
                match task.await {
                    Ok(Ok(())) => info!("Rank {} completed successfully", rank),
                    Ok(Err(e)) => {
                        error!("Rank {} failed: {}", rank, e);
                        all_success = false;
                    }
                    Err(e) => {
                        error!("Task for rank {} panicked: {}", rank, e);
                        all_success = false;
                    }
                }
            }

            if !all_success {
                bail!("One or more sidecar processes failed");
            } else {
                tracing::info!("All sidecar processes completed successfully");
            }

            Ok(())
        }
        // {
        //     info!("Starting Python sidecar for rank {}/{}", rank, world_size);
        //     run_python_sidecar(
        //         main_host, port, world_size, rank, start_rank, backend, parent_pid,
        //     )
        //     .await
        // }
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
    parent_pid: Option<u32>,
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
        .arg(device.to_string());

    if let Some(pid) = parent_pid {
        cmd.arg("--parent-pid").arg(pid.to_string());
    }

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
