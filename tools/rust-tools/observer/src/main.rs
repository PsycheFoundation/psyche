mod app;
mod tui;
pub(crate) mod utils;
mod widgets;

use std::io::Write;
use std::path::PathBuf;
use std::process;

use clap::Parser;
use psyche_event_sourcing::timeline::{ClusterTimeline, LoadProgress};
use psyche_tui::{LogOutput, logging};

use crate::utils::fmt_bytes;

/// Observer — scrub through a recorded psyche event timeline.
#[derive(Parser, Debug)]
#[command(name = "observer", version, about)]
struct Cli {
    events_dir: PathBuf,
}

const BAR_WIDTH: usize = 40;

fn print_progress(p: &LoadProgress) {
    let pct = (p.fraction * 100.0).min(100.0);
    let filled = (p.fraction * BAR_WIDTH as f32) as usize;
    let empty = BAR_WIDTH.saturating_sub(filled);

    let detail = match p.phase {
        "scanning" => format!("{} files, {}", p.files, fmt_bytes(p.total_bytes)),
        "reading" => format!(
            "{} / {}  ({} events)",
            fmt_bytes(p.bytes_read),
            fmt_bytes(p.total_bytes),
            p.entries,
        ),
        "sorting" => format!("{} events", p.entries),
        "indexing" => format!("{} events", p.entries),
        _ => String::new(),
    };

    eprint!(
        "\r  {:<9} [{}{}] {:3.0}%  {}",
        p.phase,
        "#".repeat(filled),
        " ".repeat(empty),
        pct,
        detail,
    );
    let _ = std::io::stderr().flush();
}

fn main() {
    let _logger = logging()
        .with_output(LogOutput::TUIAndConsole)
        .init()
        .expect("failed to init logging");

    let cli = Cli::parse();

    if !cli.events_dir.exists() {
        eprintln!(
            "Error: events directory does not exist: {}",
            cli.events_dir.display()
        );
        process::exit(1);
    }

    // Load whatever events exist now; the observer will live-refresh as new ones arrive.
    let timeline =
        match ClusterTimeline::from_events_dir_with_progress(&cli.events_dir, print_progress) {
            Ok(t) => {
                // Clear the progress line.
                eprint!("\r{}\r", " ".repeat(80));
                let _ = std::io::stderr().flush();
                t
            }
            Err(e) => {
                eprintln!("\nError reading events directory: {}", e);
                process::exit(1);
            }
        };

    let app = app::App::new(timeline);
    if let Err(e) = tui::run(app) {
        eprintln!("TUI error: {}", e);
        process::exit(1);
    }
}
