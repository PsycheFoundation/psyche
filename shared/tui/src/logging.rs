use std::{fs::OpenOptions, path::PathBuf};

use crate::CustomWidget;
use clap::ValueEnum;
use console_subscriber::ConsoleLayer;
use crossterm::event::{Event, KeyCode, MouseEventKind};
use logfire::{bridges::tracing::LogfireTracingPendingSpanNotSentLayer, config::AdvancedOptions};
use opentelemetry_sdk::Resource;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Widget},
};
use tracing::Level;
use tracing_subscriber::{filter::FromEnvError, fmt, layer::SubscriberExt, EnvFilter, Layer};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetEvent, TuiWidgetState};

#[derive(Clone, Debug, Copy, ValueEnum, PartialEq)]
pub enum LogOutput {
    TUI,
    Console,
    Json,
}

pub struct ShutdownHandler {
    handler: Option<logfire::ShutdownHandler>,
}

impl ShutdownHandler {
    pub fn shutdown(self) -> Result<(), logfire::ConfigureError> {
        if let Some(handler) = self.handler {
            handler.shutdown()
        } else {
            Ok(())
        }
    }
    pub fn tracer(&self) -> Option<opentelemetry_sdk::trace::Tracer> {
        self.handler.as_ref().map(|t| t.tracer.tracer().clone())
    }
}

pub fn init_logging(
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    allow_remote_logs: bool,
    service_name: Option<String>,
) -> anyhow::Result<ShutdownHandler> {
    let logfire_handler = if std::env::var("LOGFIRE_TOKEN").is_ok() && allow_remote_logs {
        Some({
            let mut builder = logfire::configure()
                .install_panic_handler()
                .with_console(None);
            // .with_metrics(Some(
            //     MetricsOptions::default().with_additional_reader(reader),
            // ))
            if let Some(service_name) = service_name {
                builder = builder.with_advanced_options(
                    AdvancedOptions::default().with_resource(
                        Resource::builder_empty()
                            .with_service_name(service_name)
                            .build(),
                    ),
                )
            }
            builder.finish()?
        })
    } else {
        None
    };

    // exclude tokio traces from regular output
    let output_logs_filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env()?
        .add_directive("tokio=off".parse().unwrap())
        .add_directive("runtime=off".parse().unwrap());

    let make_detailed_logs_filter = || -> Result<EnvFilter, FromEnvError> {
        let filter = if std::env::var("WRITE_RUST_LOG").is_ok() {
            EnvFilter::builder()
                .with_env_var("WRITE_RUST_LOG")
                .from_env()?
        } else {
            EnvFilter::builder()
                .with_default_directive(level.into())
                .from_env()?
        };
        Ok(filter
            .add_directive("tokio=off".parse().unwrap())
            .add_directive("runtime=off".parse().unwrap()))
    };

    let subscriber =
        tracing_subscriber::registry().with(ConsoleLayer::builder().with_default_env().spawn());

    let tracer = logfire_handler.as_ref().map(|t| t.tracer.tracer().clone());
    let subscriber = match output {
        LogOutput::TUI => subscriber.with(
            tui_logger::tracing_subscriber_layer()
                .with_filter(output_logs_filter)
                .boxed(),
        ),
        LogOutput::Console => subscriber.with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(output_logs_filter)
                .boxed(),
        ),
        LogOutput::Json => subscriber.with(
            fmt::layer()
                .json()
                .with_ansi(true)
                .with_writer(std::io::stdout)
                .flatten_event(true)
                .with_current_span(true)
                .with_filter(output_logs_filter)
                .boxed(),
        ),
    };

    // TODO - can we type-erase the subscribers somehow?
    // all this duplication is super ugly.
    if let Some(dir) = write_logs_file {
        let log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(dir)
            .unwrap();
        let subscriber = subscriber.with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(log_file)
                .with_filter(make_detailed_logs_filter()?),
        );

        if let Some(tracer) = tracer {
            tracing::subscriber::set_global_default(
                subscriber
                    .with(
                        LogfireTracingPendingSpanNotSentLayer
                            .with_filter(make_detailed_logs_filter()?),
                    )
                    .with(
                        tracing_opentelemetry::layer()
                            .with_error_records_to_exceptions(true)
                            .with_tracer(tracer.clone())
                            .with_filter(make_detailed_logs_filter()?),
                    )
                    .with(
                        logfire::bridges::tracing::LogfireTracingLayer(tracer.clone())
                            .with_filter(make_detailed_logs_filter()?),
                    ),
            )
        } else {
            tracing::subscriber::set_global_default(subscriber)
        }
    } else if let Some(tracer) = tracer {
        tracing::subscriber::set_global_default(
            subscriber
                .with(
                    LogfireTracingPendingSpanNotSentLayer.with_filter(make_detailed_logs_filter()?),
                )
                .with(
                    tracing_opentelemetry::layer()
                        .with_error_records_to_exceptions(true)
                        .with_tracer(tracer.clone())
                        .with_filter(make_detailed_logs_filter()?),
                )
                .with(
                    logfire::bridges::tracing::LogfireTracingLayer(tracer.clone())
                        .with_filter(make_detailed_logs_filter()?),
                ),
        )
    } else {
        tracing::subscriber::set_global_default(subscriber)
    }?;

    let shutdown_handler = ShutdownHandler {
        handler: logfire_handler,
    };
    Ok(shutdown_handler)
}

#[derive(Default)]
pub struct LoggerWidget {
    state: TuiWidgetState,
    separator: Option<char>,
    timestamp_format: Option<String>,
    show_target: Option<bool>,
}

impl LoggerWidget {
    pub fn new() -> Self {
        Self {
            state: TuiWidgetState::new(),
            separator: None,
            timestamp_format: None,
            show_target: None,
        }
    }

    pub fn with_separator(mut self, separator: char) -> Self {
        self.separator = Some(separator);
        self
    }

    pub fn with_timestamp_format(mut self, format: String) -> Self {
        self.timestamp_format = Some(format);
        self
    }

    pub fn with_show_target_field(mut self, show: bool) -> Self {
        self.show_target = Some(show);
        self
    }
}

impl CustomWidget for LoggerWidget {
    type Data = ();

    fn on_ui_event(&mut self, event: &Event) {
        match event {
            Event::Key(key) => {
                if key.code == KeyCode::Esc {
                    self.state.transition(TuiWidgetEvent::EscapeKey);
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.state.transition(TuiWidgetEvent::PrevPageKey);
                }
                MouseEventKind::ScrollDown => {
                    self.state.transition(TuiWidgetEvent::NextPageKey);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, _state: &Self::Data) {
        let mut widget = TuiLoggerWidget::default()
            .block(Block::bordered().title("Logs"))
            .output_level(Some(TuiLoggerLevelOutput::Long))
            .output_file(false)
            .output_line(false)
            .state(&self.state);

        if let Some(separator) = self.separator {
            widget = widget.output_separator(separator);
        }

        if let Some(timestamp_format) = &self.timestamp_format {
            widget = widget.output_timestamp(Some(timestamp_format.clone()));
        }

        if let Some(show_target) = self.show_target {
            widget = widget.output_target(show_target);
        }

        widget.render(area, buf);
    }
}
