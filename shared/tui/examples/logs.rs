use std::time::Duration;

use minimal::MinimalWidget;
use psyche_tui::{logging, logging::LoggerWidget, start_render_loop, CustomWidget};
use rand::RngCore;
use ratatui::layout::{Constraint, Direction, Layout};
use tokio::{select, time::interval};
use tracing::{error, info, warn};
mod minimal;

struct MinimalAndLogs {
    minimal: MinimalWidget,
    logger: LoggerWidget,
}

impl MinimalAndLogs {
    fn new() -> Self {
        Self {
            logger: LoggerWidget::new()
                .with_separator('|')
                .with_show_target_field(true),
            minimal: Default::default(),
        }
    }
}

impl CustomWidget for MinimalAndLogs {
    type Data = <MinimalWidget as CustomWidget>::Data;

    fn render(
        &mut self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    // minimal widget
                    Constraint::Percentage(20),
                    // logs
                    Constraint::Percentage(80),
                ]
                .as_ref(),
            )
            .split(area);
        self.minimal.render(chunks[0], buf, state);
        self.logger.render(chunks[1], buf, &Default::default());
    }
}

#[allow(dead_code)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logger = logging().with_output(psyche_tui::LogOutput::TUI).init()?;

    let (cancel, tx) = start_render_loop(MinimalAndLogs::new())?;
    let mut interval = interval(Duration::from_secs(2));

    loop {
        select! {
            _ = cancel.cancelled() => {
                break;
            }
            _ = interval.tick() => {
                let prng_num = rand::thread_rng().next_u64();
                tx.send(prng_num).await.expect("sending works!");

                info!("foo");
                warn!("bar");
                error!("baz");
            }
        }
    }

    logger.shutdown()?;

    Ok(())
}
