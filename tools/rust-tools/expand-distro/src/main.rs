use std::io::{self, Write};

use clap::Parser;
use psyche_modeling::{CompressDCT, DistroResult};
use psyche_network::distro_results_from_reader;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)] // it's only used for generating the docs correctly.
#[derive(Parser, Debug)]
enum Commands {
    // Expands a distro result from stdin.
    Expand {
        #[clap(long, default_value_t = false)]
        cpu: bool,
    },
    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "python")]
    psyche_python_extension::init_embedded_python();

    let args = Args::parse();
    let cpu = match args.command {
        Commands::PrintAllHelp { markdown } => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<Args>();

            return Ok(());
        }
        Commands::Expand { cpu } => cpu,
    };
    let target_type = tch::Kind::BFloat16;
    let target_device = if cpu {
        tch::Device::Cpu
    } else {
        tch::Device::cuda_if_available()
    };

    let distro_results_iter = distro_results_from_reader(io::stdin());

    for serialized_result in distro_results_iter {
        let mut result: DistroResult = (&serialized_result?).try_into()?;
        result.sparse_idx = result.sparse_idx.to_device(target_device);
        result.sparse_val = result.sparse_val.to_device(target_device);
        let decompressed = CompressDCT::decompress(
            &result.sparse_idx,
            &result.sparse_val,
            &result.xshape,
            result.totalk,
            target_type,
            target_device,
        );

        let flat: Vec<f32> = (&decompressed.flatten(0, -1)).try_into()?;
        let bytes = flat.into_iter().map(|f| f.to_le_bytes());
        for byte in bytes {
            std::io::stdout().write_all(&byte)?;
        }
    }
    Ok(())
}
