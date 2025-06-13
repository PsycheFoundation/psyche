use std::{fs::OpenOptions, path::PathBuf, time::Duration};

use crate::CustomWidget;
use clap::ValueEnum;
use console_subscriber::ConsoleLayer;
use crossterm::event::{Event, KeyCode, MouseEventKind};
use logfire::{
    bridges::tracing::LogfireTracingPendingSpanNotSentLayer,
    config::{AdvancedOptions, MetricsOptions},
};
use opentelemetry_sdk::{
    error::OTelSdkResult,
    metrics::{
        data::ResourceMetrics, exporter::PushMetricExporter, PeriodicReader, SdkMeterProvider,
        Temporality,
    },
    Resource,
};
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
    logfire_handler: Option<logfire::ShutdownHandler>,
    metrics_handler: Option<SdkMeterProvider>,
}

impl ShutdownHandler {
    pub fn shutdown(self) -> anyhow::Result<()> {
        if let Some(handler) = self.logfire_handler {
            handler.shutdown()?;
        }
        if let Some(handler) = self.metrics_handler {
            handler.shutdown()?;
        }
        Ok(())
    }
    pub fn tracer(&self) -> Option<opentelemetry_sdk::trace::Tracer> {
        self.logfire_handler
            .as_ref()
            .map(|t| t.tracer.tracer().clone())
    }
}

pub struct LoggingBuilder<R = NoMetrics> {
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    allow_remote_logs: bool,
    service_name: Option<String>,
    metrics_exporter: R,
}

pub struct NoMetrics;

pub struct WithMetrics<R>(pub R);

impl LoggingBuilder<NoMetrics> {
    /// Create a new logging builder with default settings
    pub fn new() -> Self {
        Self {
            output: LogOutput::Console,
            level: Level::INFO,
            write_logs_file: None,
            allow_remote_logs: false,
            service_name: None,
            metrics_exporter: NoMetrics,
        }
    }
}

impl<R> LoggingBuilder<R> {
    /// Set the log output format
    pub fn with_output(mut self, output: LogOutput) -> Self {
        self.output = output;
        self
    }

    /// Set the log level
    pub fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Set the log file path (optional)
    pub fn with_log_file<P: Into<Option<PathBuf>>>(mut self, path: P) -> Self {
        self.write_logs_file = path.into();
        self
    }

    /// Enable or disable remote logging
    pub fn with_remote_logs(mut self, allow: bool) -> Self {
        self.allow_remote_logs = allow;
        self
    }

    /// Set the service name for telemetry
    pub fn with_service_name<S: Into<String>>(mut self, name: S) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Add a metrics reader for OpenTelemetry integration
    pub fn with_metrics_exporter<ME: PushMetricExporter + 'static>(
        self,
        exporter: ME,
    ) -> LoggingBuilder<WithMetrics<ME>> {
        LoggingBuilder {
            output: self.output,
            level: self.level,
            write_logs_file: self.write_logs_file,
            allow_remote_logs: self.allow_remote_logs,
            service_name: self.service_name,
            metrics_exporter: WithMetrics(exporter),
        }
    }
}

impl LoggingBuilder<NoMetrics> {
    /// Initialize logging without metrics
    pub fn init(self) -> anyhow::Result<ShutdownHandler> {
        init_logging_impl::<DummyExporter>(
            self.output,
            self.level,
            self.write_logs_file,
            self.allow_remote_logs,
            self.service_name,
            None,
        )
    }
}

impl<R: PushMetricExporter + 'static> LoggingBuilder<WithMetrics<R>> {
    /// Initialize logging with metrics
    pub fn init(self) -> anyhow::Result<ShutdownHandler> {
        init_logging_impl(
            self.output,
            self.level,
            self.write_logs_file,
            self.allow_remote_logs,
            self.service_name,
            Some(self.metrics_exporter.0),
        )
    }
}

/// Create a new logging builder
pub fn logging() -> LoggingBuilder<NoMetrics> {
    LoggingBuilder::new()
}

/// Exists for type-safety - when you don't specify a metrics exporter, this type is used,
/// but this can't ever be constructed.
#[derive(Debug)]
enum DummyExporter {}
impl PushMetricExporter for DummyExporter {
    fn export<'a, 'b, 'c>(
        &'a self,
        _metrics: &'b mut ResourceMetrics,
    ) -> ::core::pin::Pin<
        Box<dyn ::core::future::Future<Output = OTelSdkResult> + ::core::marker::Send + 'c>,
    >
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        unreachable!()
    }

    fn force_flush<'a, 'b>(
        &'a self,
    ) -> ::core::pin::Pin<
        Box<dyn ::core::future::Future<Output = OTelSdkResult> + ::core::marker::Send + 'b>,
    >
    where
        'a: 'b,
        Self: 'b,
    {
        unreachable!()
    }

    fn shutdown(&self) -> OTelSdkResult {
        unreachable!()
    }

    fn temporality(&self) -> Temporality {
        unreachable!()
    }
}

fn init_logging_impl<E: PushMetricExporter + 'static>(
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    allow_remote_logs: bool,
    service_name: Option<String>,
    metrics_exporter: Option<E>,
) -> anyhow::Result<ShutdownHandler> {
    let logfire_enabled = std::env::var("LOGFIRE_TOKEN").is_ok() && allow_remote_logs;

    let (logfire_handler, standalone_metrics_handler) = if logfire_enabled {
        (
            Some({
                // If we have an additional metrics exporter, add it to Logfire
                let metrics_options = if let Some(exporter) = metrics_exporter {
                    let metrics_reader = PeriodicReader::builder(exporter)
                        .with_interval(Duration::from_secs(15))
                        .build();

                    MetricsOptions::default().with_additional_reader(metrics_reader)
                } else {
                    MetricsOptions::default()
                };

                let mut builder = logfire::configure()
                    .install_panic_handler()
                    .with_console(None)
                    .with_metrics(Some(metrics_options));

                if let Some(service_name) = service_name.clone() {
                    builder = builder.with_advanced_options(
                        AdvancedOptions::default().with_resource(
                            Resource::builder_empty()
                                .with_service_name(service_name)
                                .build(),
                        ),
                    );
                }

                builder.finish()?
            }),
            None,
        )
    } else {
        (
            None,
            metrics_exporter.map(|exporter| {
                let mut resource_builder = Resource::builder_empty();
                if let Some(service_name) = service_name {
                    resource_builder = resource_builder.with_service_name(service_name);
                }
                let resource = resource_builder.build();

                let reader = PeriodicReader::builder(exporter)
                    .with_interval(Duration::from_secs(15))
                    .build();

                let meter_provider = SdkMeterProvider::builder()
                    .with_resource(resource)
                    .with_reader(reader)
                    .build();

                opentelemetry::global::set_meter_provider(meter_provider.clone());

                meter_provider
            }),
        )
    };

    init_logging_core(
        output,
        level,
        write_logs_file,
        logfire_handler,
        standalone_metrics_handler,
    )
}

fn init_logging_core(
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    logfire_handler: Option<logfire::ShutdownHandler>,
    metrics_handler: Option<SdkMeterProvider>,
) -> anyhow::Result<ShutdownHandler> {
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
        logfire_handler,
        metrics_handler,
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
