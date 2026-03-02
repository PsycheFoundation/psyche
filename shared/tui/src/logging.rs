use std::{fs::OpenOptions, path::PathBuf, time::Duration};

use crate::CustomWidget;
use clap::ValueEnum;
use crossterm::event::{Event, KeyCode, MouseEventKind};
use logfire::{
    bridges::tracing::LogfireTracingPendingSpanNotSentLayer,
    config::{AdvancedOptions, MetricsOptions},
};
use opentelemetry::{KeyValue, trace::TracerProvider};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    logs::{BatchConfigBuilder, BatchLogProcessor, SdkLoggerProvider},
    metrics::{
        PeriodicReader, SdkMeterProvider, Temporality, data::ResourceMetrics,
        exporter::PushMetricExporter,
    },
    trace::{BatchSpanProcessor, SdkTracerProvider},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Widget},
};
use tracing::Level;
use tracing_subscriber::{EnvFilter, Layer, filter::FromEnvError, fmt};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetEvent, TuiWidgetState};

#[derive(Clone, Debug, Copy, ValueEnum, PartialEq)]
pub enum LogOutput {
    TUI,
    Console,
    Json,
    None,
}

pub struct ShutdownHandler {
    handlers: Vec<Box<dyn Shutdownable>>,
}

impl ShutdownHandler {
    pub fn new(handlers: Vec<Box<dyn Shutdownable>>) -> Self {
        Self { handlers }
    }

    pub fn shutdown(self) -> anyhow::Result<()> {
        for handler in self.handlers {
            handler.shutdown()?;
        }
        Ok(())
    }
}

/// Exists for type-safety - when you don't specify a metrics exporter, this type is used,
/// but this can't ever be constructed, because it's an enum with no variants.
#[derive(Debug)]
pub enum NoMetrics {}
impl PushMetricExporter for NoMetrics {
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

pub struct LoggingBuilder {
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    remote_logs_destination: Option<RemoteLogsDestination>,
    trace_destination: Option<TraceDestination>,
    service_info: Option<ServiceInfo>,
    metrics_destination: Option<MetricsDestination>,
}

impl Default for LoggingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggingBuilder {
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

    /// Set remote logs destination
    pub fn with_remote_logs(mut self, destination: Option<RemoteLogsDestination>) -> Self {
        self.remote_logs_destination = destination;
        self
    }

    pub fn with_trace_destination(mut self, destination: Option<TraceDestination>) -> Self {
        self.trace_destination = destination;
        self
    }

    /// Set metrics destination
    pub fn with_metrics_destination(
        mut self,
        destination: Option<MetricsDestination>,
    ) -> LoggingBuilder {
        self.metrics_destination = destination;
        self
    }

    /// Set the service info for telemetry
    pub fn with_service_info(mut self, info: ServiceInfo) -> Self {
        self.service_info = Some(info);
        self
    }
}

impl LoggingBuilder {
    pub fn new() -> Self {
        Self {
            output: LogOutput::Console,
            level: Level::INFO,
            write_logs_file: None,
            remote_logs_destination: None,
            service_info: None,
            metrics_destination: None,
            trace_destination: None,
        }
    }
    pub fn init(self) -> anyhow::Result<ShutdownHandler> {
        init_logging_impl(
            self.output,
            self.level,
            self.write_logs_file,
            self.service_info,
            self.remote_logs_destination,
            self.metrics_destination,
            self.trace_destination,
        )
    }
}

/// Create a new logging builder
pub fn logging() -> LoggingBuilder {
    LoggingBuilder::new()
}

pub trait Shutdownable {
    fn shutdown(&self) -> anyhow::Result<()>;
}

impl Shutdownable for logfire::ShutdownHandler {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(logfire::ShutdownHandler::shutdown(self)?)
    }
}

impl Shutdownable for SdkMeterProvider {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(SdkMeterProvider::shutdown(self)?)
    }
}

impl Shutdownable for SdkTracerProvider {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(SdkTracerProvider::shutdown(self)?)
    }
}

pub struct OtelMetricsHandler {
    provider: SdkMeterProvider,
}

impl Shutdownable for OtelMetricsHandler {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(self.provider.shutdown()?)
    }
}

pub struct OtelTracingHandler {
    provider: SdkTracerProvider,
    tracer: opentelemetry_sdk::trace::Tracer,
}

pub struct OtelLoggerHandler {
    provider: SdkLoggerProvider,
}

impl Shutdownable for OtelTracingHandler {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(self.provider.shutdown()?)
    }
}

impl Shutdownable for OtelLoggerHandler {
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(self.provider.shutdown()?)
    }
}

#[derive(Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub instance_id: String,
    pub namespace: String,
    pub deployment_environment: String,
    pub run_id: Option<String>,
}

impl ServiceInfo {
    pub fn into_attributes(self) -> [KeyValue; 5] {
        [
            KeyValue::new("service.name", self.name),
            KeyValue::new("service.instance.id", self.instance_id),
            KeyValue::new("service.namespace", self.namespace),
            KeyValue::new("deployment.environment.name", self.deployment_environment),
            KeyValue::new("run.id", self.run_id.unwrap_or("".to_string())),
        ]
    }
}

fn create_otel_metrics_handler(
    config: &OpenTelemetry,
    service_name: Option<ServiceInfo>,
) -> anyhow::Result<OtelMetricsHandler> {
    let mut exporter_builder = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_endpoint(&config.endpoint);

    if let Some(header) = &config.authorization_header {
        exporter_builder = exporter_builder.with_headers(std::collections::HashMap::from([(
            "authorization".to_string(),
            header.to_string(),
        )]));
    }

    let exporter = exporter_builder.build()?;

    let mut resource_builder = Resource::builder_empty();
    if let Some(info) = service_name {
        resource_builder = resource_builder.with_attributes(info.into_attributes())
    }
    let resource = resource_builder.build();

    let reader = PeriodicReader::builder(exporter)
        .with_interval(config.report_interval)
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(reader)
        .build();

    opentelemetry::global::set_meter_provider(provider.clone());

    Ok(OtelMetricsHandler { provider })
}

fn create_otel_tracing_handler(
    config: &OpenTelemetry,
    service_info: Option<ServiceInfo>,
) -> anyhow::Result<OtelTracingHandler> {
    let mut exporter_builder = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(&config.endpoint);

    if let Some(header) = &config.authorization_header {
        exporter_builder = exporter_builder.with_headers(std::collections::HashMap::from([(
            "authorization".to_string(),
            header.to_string(),
        )]));
    }

    let trace_exporter = exporter_builder.build()?;

    let mut resource_builder = Resource::builder_empty();
    if let Some(service_info) = service_info.clone() {
        resource_builder = resource_builder.with_attributes(service_info.into_attributes());
    }
    let resource = resource_builder.build();

    let batch_processor = BatchSpanProcessor::builder(trace_exporter).build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_span_processor(batch_processor)
        .build();

    let tracer = provider.tracer(
        service_info
            .map(|s| s.name)
            .unwrap_or_else(|| "rust-app".to_string()),
    );

    Ok(OtelTracingHandler { provider, tracer })
}

pub fn create_otel_logger_handler(
    config: &OpenTelemetry,
    service_info: Option<ServiceInfo>,
) -> anyhow::Result<OtelLoggerHandler> {
    let mut exporter_builder = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_endpoint(&config.endpoint);

    if let Some(header) = &config.authorization_header {
        exporter_builder = exporter_builder.with_headers(std::collections::HashMap::from([(
            "authorization".to_string(),
            header.to_string(),
        )]));
    }

    let logger_exporter = exporter_builder.build()?;

    let mut resource_builder = Resource::builder_empty();
    if let Some(service_info) = service_info {
        resource_builder = resource_builder.with_attributes(service_info.into_attributes());
    }
    let resource = resource_builder.build();

    let provider = SdkLoggerProvider::builder()
        .with_log_processor(
            BatchLogProcessor::builder(logger_exporter)
                .with_batch_config(
                    BatchConfigBuilder::default()
                        .with_max_queue_size(150_000)
                        .build(),
                )
                .build(),
        )
        .with_resource(resource)
        .build();

    Ok(OtelLoggerHandler { provider })
}

#[derive(Clone)]
pub struct Logfire {
    pub api_key: String,
}

#[derive(Clone)]
pub struct OpenTelemetry {
    pub endpoint: String,
    pub authorization_header: Option<String>,
    pub report_interval: Duration,
}

pub enum MetricsDestination {
    Logfire(Logfire),
    OpenTelemetry(OpenTelemetry),
}

#[derive(Clone)]
pub enum RemoteLogsDestination {
    Logfire(Logfire),
    OpenTelemetry(OpenTelemetry),
}

#[derive(Clone)]
pub enum TraceDestination {
    OpenTelemetry(OpenTelemetry),
    LogFire(Logfire),
}

fn init_logging_impl(
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    service_info: Option<ServiceInfo>,
    remote_logs_destination: Option<RemoteLogsDestination>,
    metrics_destination: Option<MetricsDestination>,
    trace_destination: Option<TraceDestination>,
) -> anyhow::Result<ShutdownHandler> {
    let mut shutdown_handlers: Vec<Box<dyn Shutdownable>> = vec![];

    let logfire_handles_logs = matches!(
        remote_logs_destination,
        Some(RemoteLogsDestination::Logfire(_))
    );
    let logfire_handles_traces = matches!(trace_destination, Some(TraceDestination::LogFire(_)));
    let logfire_handles_metrics =
        matches!(metrics_destination, Some(MetricsDestination::Logfire(_)));

    let (logfire_handler, logfire_tracer) =
        if logfire_handles_logs || logfire_handles_traces || logfire_handles_metrics {
            let api_key =
                if let Some(RemoteLogsDestination::Logfire(config)) = &remote_logs_destination {
                    &config.api_key
                } else if let Some(TraceDestination::LogFire(config)) = &trace_destination {
                    &config.api_key
                } else if let Some(MetricsDestination::Logfire(config)) = &metrics_destination {
                    &config.api_key
                } else {
                    return Err(anyhow::anyhow!("Logfire configuration not found"));
                };

            std::env::set_var("LOGFIRE_TOKEN", api_key);

            let mut builder = logfire::configure()
                .install_panic_handler()
                .with_console(None);

            // Enable metrics if Logfire handles them
            if logfire_handles_metrics {
                builder = builder.with_metrics(Some(MetricsOptions::default()));
            }

            if let Some(service_name) = service_info.clone() {
                builder = builder.with_advanced_options(
                    AdvancedOptions::default().with_resource(
                        Resource::builder_empty()
                            .with_attributes(service_name.into_attributes())
                            .build(),
                    ),
                );
            }

            let handler = builder.finish()?;
            let tracer = handler.tracer.tracer().clone();
            (Some(handler), Some(tracer))
        } else {
            (None, None)
        };

    // Handle OpenTelemetry logs if not using Logfire
    let (otel_logger_handler, otel_logger) = if !logfire_handles_logs {
        if let Some(RemoteLogsDestination::OpenTelemetry(otel_config)) = &remote_logs_destination {
            let handler = create_otel_logger_handler(otel_config, service_info.clone())?;
            let logger = handler.provider.clone();
            (Some(handler), Some(logger))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Handle OpenTelemetry tracing if not using Logfire
    let (otel_tracing_handler, otel_tracer) = if !logfire_handles_traces {
        if let Some(TraceDestination::OpenTelemetry(otel_config)) = &trace_destination {
            let handler = create_otel_tracing_handler(otel_config, service_info.clone())?;
            let tracer = handler.tracer.clone();
            (Some(handler), Some(tracer))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Handle OpenTelemetry metrics if not using Logfire
    let otel_metrics_handler = if !logfire_handles_metrics {
        if let Some(MetricsDestination::OpenTelemetry(otel_config)) = &metrics_destination {
            Some(create_otel_metrics_handler(
                otel_config,
                service_info.clone(),
            )?)
        } else {
            None
        }
    } else {
        None
    };

    let tracer = logfire_tracer.or(otel_tracer);

    // Otel logger works separately from the tracer, logfire's does not.
    let logger = otel_logger;

    // Add handlers to shutdown list
    if let Some(handler) = logfire_handler {
        shutdown_handlers.push(Box::new(handler));
    }
    if let Some(handler) = otel_logger_handler {
        shutdown_handlers.push(Box::new(handler));
    }
    if let Some(handler) = otel_tracing_handler {
        shutdown_handlers.push(Box::new(handler));
    }
    if let Some(handler) = otel_metrics_handler {
        shutdown_handlers.push(Box::new(handler));
    }

    init_logging_core(
        output,
        level,
        write_logs_file,
        tracer,
        logger,
        logfire_handles_logs,
    )?;

    Ok(ShutdownHandler::new(shutdown_handlers))
}

fn init_logging_core(
    output: LogOutput,
    level: Level,
    write_logs_file: Option<PathBuf>,
    tracer: Option<opentelemetry_sdk::trace::Tracer>,
    logger: Option<opentelemetry_sdk::logs::SdkLoggerProvider>,
    is_logfire: bool,
) -> anyhow::Result<()> {
    use tracing_subscriber::layer::SubscriberExt;

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

    let mut layers: Vec<Box<dyn tracing_subscriber::Layer<_> + Send + Sync>> = Vec::new();

    // add output layer
    match output {
        LogOutput::TUI => layers.push(
            tui_logger::tracing_subscriber_layer()
                .with_filter(output_logs_filter)
                .boxed(),
        ),
        LogOutput::Console => layers.push(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(output_logs_filter)
                .boxed(),
        ),
        LogOutput::Json => layers.push(
            fmt::layer()
                .json()
                .with_ansi(true)
                .with_writer(std::io::stdout)
                .flatten_event(true)
                .with_current_span(true)
                .with_filter(output_logs_filter)
                .boxed(),
        ),
        LogOutput::None => {}
    }

    // add file layer
    if let Some(log_file_path) = write_logs_file {
        let log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_file_path)?;

        layers.push(
            fmt::layer()
                .with_ansi(false)
                .with_writer(log_file)
                .with_filter(make_detailed_logs_filter()?)
                .boxed(),
        );
    }

    // add OpenTelemetry layer
    if let Some(tracer) = tracer.clone() {
        layers.push(
            tracing_opentelemetry::layer()
                .with_error_records_to_exceptions(true)
                .with_tracer(tracer.clone())
                .with_filter(make_detailed_logs_filter()?)
                .boxed(),
        );

        // add Logfire layers
        if is_logfire {
            layers.push(
                LogfireTracingPendingSpanNotSentLayer
                    .with_filter(make_detailed_logs_filter()?)
                    .boxed(),
            );
            layers.push(
                logfire::bridges::tracing::LogfireTracingLayer(tracer)
                    .with_filter(make_detailed_logs_filter()?)
                    .boxed(),
            );
        }
    }

    if let Some(logger) = logger {
        layers.push(
            opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger)
                .with_filter(make_detailed_logs_filter()?)
                .boxed(),
        );
    }

    // Forward error! events to the EventStore so they appear in event files.
    layers.push(psyche_event_sourcing::EventStoreTracingLayer.boxed());

    // build all into one subscriber, set as global default
    let subscriber = tracing_subscriber::registry().with(layers);
    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
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
