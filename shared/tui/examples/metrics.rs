use std::time::Duration;

use psyche_tui::{
    LogOutput, ServiceInfo,
    logging::{
        MetricsDestination, OpenTelemetry, RemoteLogsDestination, TraceDestination, logging,
    },
};
use tracing::{Level, info, span};

#[tokio::main]
async fn main() {
    let authorization_header =
        std::env::var("OTLP_AUTH_HEADER").expect("env var OTLP_AUTH_HEADER not set");
    let metrics_endpoint =
        std::env::var("OTLP_METRICS_URL").expect("env var OTLP_METRICS_URL not set");

    let tracing_endpoint =
        std::env::var("OTLP_TRACING_URL").expect("env var OTLP_TRACING_URL not set");

    let log_endpoint = std::env::var("OTLP_LOGS_URL").expect("env var OTLP_LOGS_URL not set");

    let _logs = logging()
        .with_output(LogOutput::Console)
        .with_level(Level::INFO)
        .with_service_info(ServiceInfo {
            name: "metrics-example".to_string(),
            instance_id: "local".to_string(),
            namespace: "psyche".to_string(),
            deployment_environment: "development".to_string(),
            run_id: Some("run_id_test".to_string()),
        })
        .with_metrics_destination(Some(MetricsDestination::OpenTelemetry(OpenTelemetry {
            endpoint: metrics_endpoint,
            authorization_header: Some(authorization_header.clone()),
            report_interval: Duration::from_secs(1),
        })))
        .with_remote_logs(Some(RemoteLogsDestination::OpenTelemetry(OpenTelemetry {
            endpoint: log_endpoint,
            authorization_header: Some(authorization_header.clone()),
            report_interval: Duration::from_secs(1),
        })))
        .with_trace_destination(Some(TraceDestination::OpenTelemetry(OpenTelemetry {
            endpoint: tracing_endpoint,
            authorization_header: Some(authorization_header),
            report_interval: Duration::from_secs(1),
        })))
        .init()
        .unwrap();

    let meter = opentelemetry::global::meter("test-app");
    let counter = meter.u64_counter("startup_counter").build();
    counter.add(1, &[]);

    let meter = opentelemetry::global::meter("test-app");
    let counter = meter.u64_counter("test_metrics").build();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;
        counter.add(1, &[]);

        // Test tracing
        let root_span = span!(Level::INFO, "Span test",);
        let _enter = root_span.enter();

        // Test logging
        info!(bananas = "yummy", "Sample log output!");
    }
}
