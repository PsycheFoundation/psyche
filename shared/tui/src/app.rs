use crate::{terminal::TerminalWrapper, widget::CustomWidget};
use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use std::time::Duration;
use tokio::{
    select,
    sync::mpsc::{self, Receiver},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

#[derive(Debug)]
enum AppEvent<S> {
    UiEvent(Event),
    StateUpdated(S),
    Frame,
}

pub struct App<W: CustomWidget> {
    custom_widget: W,
    custom_widget_data_state: W::Data,
}

impl<W: CustomWidget> App<W> {
    pub fn new(widget: W) -> Self {
        Self {
            custom_widget: widget,
            custom_widget_data_state: Default::default(),
        }
    }

    pub async fn start(
        mut self,
        shutdown_token: CancellationToken,
        mut terminal: TerminalWrapper<impl Backend>,
        mut state_rx: Receiver<W::Data>,
    ) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel(10);

        tokio::spawn({
            let tx = tx.clone();
            let shutdown_token = shutdown_token.clone();
            async move {
                let mut interval = tokio::time::interval(Duration::from_millis(150));
                loop {
                    select! {
                        _ = shutdown_token.cancelled() => {
                            break;
                        }
                        _ = interval.tick() => {
                            if tx.send(AppEvent::Frame).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        tokio::spawn({
            let tx = tx.clone();
            let shutdown_token = shutdown_token.clone();
            let mut reader = EventStream::new();
            async move {
                loop {
                    select! {
                        _ = shutdown_token.cancelled() => {
                            break;
                        }
                        Some(Ok(event)) = reader.next() => {
                            trace!(target:"crossterm", "Stdin event received {:?}", event);
                            if tx.send(AppEvent::UiEvent(event)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        tokio::spawn({
            let tx = tx.clone();
            let shutdown_token = shutdown_token.clone();
            async move {
                loop {
                    select! {
                        _ = shutdown_token.cancelled() => {
                            break;
                        }
                        Some(state) = state_rx.recv() => {
                            if tx.send(AppEvent::StateUpdated(state)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        loop {
            select! {
                _ = shutdown_token.cancelled() => {
                    break;
                }
                Some(event) = rx.recv() => {
                    match event {
                        AppEvent::UiEvent(event) => self.handle_ui_event(event, shutdown_token.clone()),
                        AppEvent::StateUpdated(s) => {
                            self.custom_widget_data_state = s;
                        }
                        AppEvent::Frame => {
                            // just render!
                        }
                    }
                    self.draw(&mut terminal.0)?;
                }
            }
        }
        Ok(())
    }

    fn handle_ui_event(&mut self, event: Event, shutdown_token: CancellationToken) {
        debug!(target: "App", "Handling UI event: {:?}",event);

        self.custom_widget.on_ui_event(&event);

        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('q') => shutdown_token.cancel(),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    shutdown_token.cancel()
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        terminal.draw(|frame| {
            self.custom_widget.render(
                frame.area(),
                frame.buffer_mut(),
                &self.custom_widget_data_state,
            );
        })?;
        Ok(())
    }
}
