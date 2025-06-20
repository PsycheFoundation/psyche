mod app;
pub mod logging;
mod maybe;
mod tabbed;
mod terminal;
mod widget;

use anyhow::Result;
use terminal::init_terminal;
use tokio::{
    signal,
    sync::mpsc::{self, Sender},
};
use tokio_util::sync::CancellationToken;

pub use app::App;
pub use logging::{logging, LogOutput, ServiceInfo};
pub use maybe::MaybeTui;
pub use tabbed::TabbedWidget;
pub use widget::CustomWidget;

pub fn start_render_loop<T: CustomWidget>(
    widget: T,
) -> Result<(CancellationToken, Sender<T::Data>)> {
    let (tx, rx) = mpsc::channel(10);
    let cancel = CancellationToken::new();
    tokio::spawn({
        let cancel = cancel.clone();
        async move {
            let terminal = init_terminal().unwrap();
            let start_result = App::new(widget).start(cancel, terminal, rx).await;
            start_result.unwrap();
            println!("explicit shutdown :)")
        }
    });
    Ok((cancel, tx))
}

pub fn maybe_start_render_loop<T: CustomWidget>(
    widget: Option<T>,
) -> Result<(CancellationToken, Option<Sender<T::Data>>)> {
    Ok(match widget {
        Some(widget) => {
            let (cancel, tx) = start_render_loop(widget)?;
            (cancel, Some(tx))
        }
        None => (
            {
                let token = CancellationToken::new();
                tokio::spawn({
                    let token = token.clone();
                    async move {
                        signal::ctrl_c().await.unwrap();
                        token.cancel();
                    }
                });
                token
            },
            None,
        ),
    })
}

pub use crossterm;
pub use ratatui;
