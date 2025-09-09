//! Iroh metrics collector for OpenTelemetry
//!
//! This module pulls metrics from iroh-metrics
//! and pushes them to OpenTelemetry.
//!
//! # Example
//!
//! ```rust,ignore
//! use psyche_metrics::{create_iroh_registry, IrohMetricsCollector};
//!
//! let registry = create_iroh_registry();
//!
//! // register iroh metrics groups
//! // e.g. f you have iroh-blobs:
//! // registry.write().unwrap().register(blobs.metrics().clone());
//!
//! // create the collector
//! let collector = IrohMetricsCollector::new(registry.clone());
//!
//! // the collector will poll the registry every 5 seconds
//! // and push metrics to OpenTelemetry with the "iroh_" prefix
//! // until dropped.
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Gauge, Meter},
};
use tokio::time::interval;

use iroh_metrics::{MetricsSource, Registry, parse_prometheus_metrics};

#[derive(Clone, Debug)]
/// Iroh metrics collector that pulls metrics from an iroh-metrics Registry into OpenTelemetry
pub struct IrohMetricsCollector {
    iroh_registry: Arc<RwLock<Registry>>,
    collector_handle: Arc<tokio::task::JoinHandle<()>>,
}

impl Drop for IrohMetricsCollector {
    fn drop(&mut self) {
        self.collector_handle.abort();
    }
}

// track counter state for delta calculation
#[derive(Default)]
struct CounterState {
    last_value: f64,
}

impl IrohMetricsCollector {
    pub fn new(iroh_registry: Arc<RwLock<Registry>>) -> Self {
        Self::with_interval(iroh_registry, Duration::from_secs(5))
    }

    pub fn with_interval(iroh_registry: Arc<RwLock<Registry>>, interval: Duration) -> Self {
        let meter = global::meter("iroh");

        let collector_handle =
            Self::start_collection_with_interval(iroh_registry.clone(), meter.clone(), interval);

        Self {
            iroh_registry,
            collector_handle,
        }
    }

    fn start_collection_with_interval(
        registry: Arc<RwLock<Registry>>,
        meter: Meter,
        poll_interval: Duration,
    ) -> Arc<tokio::task::JoinHandle<()>> {
        let mut interval = interval(poll_interval);

        let mut counters: HashMap<String, Counter<u64>> = HashMap::new();
        let mut gauges: HashMap<String, Gauge<f64>> = HashMap::new();

        // track counter states for deltas
        let mut counter_states: HashMap<String, CounterState> = HashMap::new();

        Arc::new(tokio::spawn(async move {
            loop {
                interval.tick().await;

                // get metrics from iroh registry
                let metrics_string = match registry.read() {
                    Ok(reg) => match reg.encode_openmetrics_to_string() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!("Failed to encode iroh metrics: {}", e);
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read iroh registry: {}", e);
                        continue;
                    }
                };

                let parsed_metrics = parse_prometheus_metrics(&metrics_string);

                for (metric_name, value) in parsed_metrics {
                    let (base_name, labels) = parse_metric_name_and_labels(&metric_name);

                    let otel_labels: Vec<KeyValue> = labels
                        .into_iter()
                        .map(|(k, v)| KeyValue::new(k, v))
                        .collect();

                    // Determine if this is a counter or gauge based on name
                    // Ideally we could pull this from iroh metrics directly instead of parsing the text format,
                    // but this works for now.
                    if base_name.ends_with("_total") {
                        // It's a counter
                        let otel_name = base_name.trim_end_matches("_total");
                        let counter = counters.entry(otel_name.to_string()).or_insert_with(|| {
                            meter.u64_counter(format!("iroh_{otel_name}")).build()
                        });

                        // calculate delta for counter
                        let state = counter_states.entry(metric_name.clone()).or_default();
                        if value >= state.last_value {
                            let delta = value - state.last_value;
                            if delta > 0.0 {
                                counter.add(delta as u64, &otel_labels);
                            }
                            state.last_value = value;
                        } else {
                            // counter was reset, record the new value as-is
                            counter.add(value as u64, &otel_labels);
                            state.last_value = value;
                        }
                    } else {
                        // it's a gauge
                        let gauge = gauges.entry(base_name.clone()).or_insert_with(|| {
                            meter.f64_gauge(format!("iroh_{base_name}")).build()
                        });

                        gauge.record(value, &otel_labels);
                    }
                }
            }
        }))
    }

    /// get a reference to the internal iroh-metrics registry
    pub fn iroh_registry(&self) -> &Arc<RwLock<Registry>> {
        &self.iroh_registry
    }

    /// register an iroh MetricsGroup with this collector
    pub fn register_group(&self, group: Arc<dyn iroh_metrics::MetricsGroup>) -> Result<(), String> {
        self.iroh_registry
            .write()
            .map_err(|e| format!("Failed to acquire write lock: {e}"))?
            .register(group);
        Ok(())
    }

    /// register an iroh MetricsGroupSet with this collector
    pub fn register_group_set(
        &self,
        group_set: &impl iroh_metrics::MetricsGroupSet,
    ) -> Result<(), String> {
        self.iroh_registry
            .write()
            .map_err(|e| format!("Failed to acquire write lock: {e}"))?
            .register_all(group_set);
        Ok(())
    }
}

pub fn create_iroh_registry() -> Arc<RwLock<Registry>> {
    Arc::new(RwLock::new(Registry::default()))
}

/// Parse metric name and extract labels
/// Example: "http_requests_total{method=\"GET\",status=\"200\"}"
/// Returns: ("http_requests_total", [("method", "GET"), ("status", "200")])
fn parse_metric_name_and_labels(metric: &str) -> (String, Vec<(String, String)>) {
    if let Some(brace_pos) = metric.find('{') {
        let base_name = metric[..brace_pos].to_string();
        let labels_str = &metric[brace_pos + 1..metric.len() - 1];

        let mut labels = Vec::new();
        for label_pair in labels_str.split(',') {
            if let Some(eq_pos) = label_pair.find('=') {
                let key = label_pair[..eq_pos].trim().to_string();
                let value = label_pair[eq_pos + 1..]
                    .trim()
                    .trim_matches('"')
                    .to_string();
                labels.push((key, value));
            }
        }

        (base_name, labels)
    } else {
        (metric.to_string(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh_metrics::{Counter, Gauge};
    use std::sync::Arc;
    use tokio::time::{Duration, sleep};

    #[derive(Debug, Default, iroh_metrics::MetricsGroup)]
    #[metrics(name = "test")]
    struct TestMetrics {
        requests: Counter,
        connections: Gauge,
        errors: Counter,
    }

    #[tokio::test]
    async fn test_create_iroh_registry() {
        let registry = create_iroh_registry();

        assert!(registry.read().is_ok());
        assert!(registry.write().is_ok());

        let encoded = registry
            .read()
            .unwrap()
            .encode_openmetrics_to_string()
            .unwrap();
        assert_eq!(encoded.trim(), "# EOF");
    }

    #[tokio::test]
    async fn test_collector_creation() {
        let registry = create_iroh_registry();
        let collector = IrohMetricsCollector::new(registry.clone());

        assert!(Arc::ptr_eq(collector.iroh_registry(), &registry));
    }

    #[tokio::test]
    async fn test_register_group() {
        let registry = create_iroh_registry();
        let collector = IrohMetricsCollector::new(registry.clone());

        let test_metrics = Arc::new(TestMetrics::default());

        assert!(collector.register_group(test_metrics.clone()).is_ok());

        let encoded = registry
            .read()
            .unwrap()
            .encode_openmetrics_to_string()
            .unwrap();
        assert!(encoded.contains("test_requests"));
        assert!(encoded.contains("test_connections"));
        assert!(encoded.contains("test_errors"));

        drop(collector);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let registry = create_iroh_registry();
        let collector =
            IrohMetricsCollector::with_interval(registry.clone(), Duration::from_millis(50));

        let test_metrics = Arc::new(TestMetrics::default());
        collector.register_group(test_metrics.clone()).unwrap();

        test_metrics.requests.inc();
        test_metrics.requests.inc();
        test_metrics.connections.set(5);
        test_metrics.errors.inc_by(3);

        sleep(Duration::from_millis(100)).await;

        let encoded = registry
            .read()
            .unwrap()
            .encode_openmetrics_to_string()
            .unwrap();
        assert!(encoded.contains("test_requests_total 2"));
        assert!(encoded.contains("test_connections 5"));
        assert!(encoded.contains("test_errors_total 3"));
    }

    #[test]
    fn test_parse_metric_name_and_labels() {
        let (name, labels) = parse_metric_name_and_labels("simple_metric");
        assert_eq!(name, "simple_metric");
        assert!(labels.is_empty());

        let (name, labels) = parse_metric_name_and_labels("http_requests_total{method=\"GET\"}");
        assert_eq!(name, "http_requests_total");
        assert_eq!(labels, vec![("method".to_string(), "GET".to_string())]);

        let (name, labels) =
            parse_metric_name_and_labels("http_requests_total{method=\"GET\",status=\"200\"}");
        assert_eq!(name, "http_requests_total");
        assert_eq!(
            labels,
            vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string())
            ]
        );

        let (name, labels) =
            parse_metric_name_and_labels("metric{key1 = \"value1\" , key2= \"value2\"}");
        assert_eq!(name, "metric");
        assert_eq!(
            labels,
            vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_prometheus_metrics() {
        let prometheus_data = r#"
# HELP test_requests_total Total requests
# TYPE test_requests_total counter
test_requests_total 42
# HELP test_connections Active connections
# TYPE test_connections gauge
test_connections 5
# HELP test_errors_total Total errors
# TYPE test_errors_total counter
test_errors_total{type="timeout"} 3
test_errors_total{type="network"} 7
# EOF
"#;

        let metrics = parse_prometheus_metrics(prometheus_data);

        assert_eq!(metrics.get("test_requests_total"), Some(&42.0));
        assert_eq!(metrics.get("test_connections"), Some(&5.0));
        assert_eq!(
            metrics.get("test_errors_total{type=\"timeout\"}"),
            Some(&3.0)
        );
        assert_eq!(
            metrics.get("test_errors_total{type=\"network\"}"),
            Some(&7.0)
        );
    }

    #[tokio::test]
    async fn test_counter_delta_tracking() {
        let registry = create_iroh_registry();
        let collector =
            IrohMetricsCollector::with_interval(registry.clone(), Duration::from_millis(50));

        let test_metrics = Arc::new(TestMetrics::default());
        collector.register_group(test_metrics.clone()).unwrap();

        test_metrics.requests.inc_by(10);
        sleep(Duration::from_millis(100)).await;

        test_metrics.requests.inc_by(5);
        sleep(Duration::from_millis(100)).await;

        let encoded = registry
            .read()
            .unwrap()
            .encode_openmetrics_to_string()
            .unwrap();
        assert!(encoded.contains("test_requests_total 15"));

        drop(collector);
    }

    #[tokio::test]
    async fn test_gauge_updates() {
        let registry = create_iroh_registry();
        let collector =
            IrohMetricsCollector::with_interval(registry.clone(), Duration::from_millis(50));

        let test_metrics = Arc::new(TestMetrics::default());
        collector.register_group(test_metrics.clone()).unwrap();

        test_metrics.connections.set(10);
        sleep(Duration::from_millis(100)).await;

        test_metrics.connections.set(7);
        sleep(Duration::from_millis(100)).await;

        let encoded = registry
            .read()
            .unwrap()
            .encode_openmetrics_to_string()
            .unwrap();
        assert!(encoded.contains("test_connections 7"));
    }

    #[tokio::test]
    async fn test_collector_drop_cleanup() {
        let registry = create_iroh_registry();
        let collector = IrohMetricsCollector::new(registry.clone());

        assert!(!collector.collector_handle.is_finished());
        let handle = collector.collector_handle.clone();
        drop(collector);

        // give it a sec to cleanup
        sleep(Duration::from_millis(10)).await;

        assert!(handle.is_finished());
    }
}
