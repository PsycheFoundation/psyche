use clap::Parser;
use plotters::prelude::*;
use psyche_coordinator::{CoordinatorConfig, model_extra_data::ModelExtraData};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(args_conflicts_with_subcommands = true)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(required = true)]
    config_path: Option<PathBuf>,
}

#[allow(clippy::large_enum_variant)] // it's only used for generating the docs correctly.
#[derive(Parser, Debug)]
enum Commands {
    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[derive(Deserialize)]
struct Config {
    pub config: CoordinatorConfig,
    pub model_extra_data: ModelExtraData,
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Some(Commands::PrintAllHelp { markdown }) => {
            // This is a required argument for the time being.
            assert!(markdown);

            let () = clap_markdown::print_help_markdown::<Args>();

            return Ok(());
        }
        None => {}
    };

    let config_path = args.config_path.unwrap();

    let config: Config = toml::from_str(&std::fs::read_to_string(&config_path)?)?;

    let steps = config.config.total_steps;
    let lr = config.model_extra_data.lr_schedule;

    let root = BitMapBackend::new("lr-plot.png", (steps.min(10_000), 1024)).into_drawing_area();
    root.fill(&WHITE)?;

    let all_vals: Vec<_> = (0..steps)
        .map(|step| (step as f64, lr.get_lr(step)))
        .collect();
    let min = all_vals
        .iter()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap()
        .1;
    let max = all_vals
        .iter()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap()
        .1;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("LR of {}", config_path.display()),
            ("sans-serif", 24).into_font(),
        )
        .margin(16)
        .x_label_area_size(100)
        .y_label_area_size(100)
        .build_cartesian_2d(-0f64..(steps as f64), min..max)?;

    chart.configure_mesh().draw()?;

    chart.draw_series(LineSeries::new(all_vals, &RED))?;

    root.present()?;

    Ok(())
}
