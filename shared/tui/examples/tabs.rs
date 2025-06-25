use std::time::Duration;

use psyche_tui::{logging, start_render_loop, CustomWidget, TabbedWidget};
use rand::{seq::SliceRandom, Rng};
use ratatui::widgets::{Paragraph, Widget};
use tokio::{select, time::interval};
use tracing::{error, info, warn};

mod minimal;
use minimal::MinimalWidget;

#[derive(Default)]
struct SillyDogWidget;
impl CustomWidget for SillyDogWidget {
    type Data = String;

    fn render(
        &mut self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        Paragraph::new(format!(
            r#"
      / \__
     (    @\___
    /          O
   /    (_____/    {state}
  /_____/    U   "#,
        ))
        .render(area, buf);
    }
}

type Widgets = TabbedWidget<(SillyDogWidget, MinimalWidget)>;

const BARKS: [&str; 5] = ["bork", "woof", "boof", "bark", "hello im a dog"];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logger = logging().with_output(psyche_tui::LogOutput::TUI).init()?;

    info!("foo");
    warn!("bar");
    error!("baz");

    let (cancel, tx) = start_render_loop(Widgets::new(
        Default::default(),
        &["Silly Dog", "State Example"],
    ))?;
    let mut interval = interval(Duration::from_secs(2));

    loop {
        select! {
            _ = cancel.cancelled() => {
                break;
            }
            _ = interval.tick() => {
                let mut rng = rand::thread_rng();
                let random_num = rng.gen::<u64>();
                let bark = BARKS.choose(&mut rng).unwrap().to_string();
                let states = (bark, random_num);
                tx.send(states).await.expect("sending works!");
            }
        }
    }

    logger.shutdown()?;
    Ok(())
}
