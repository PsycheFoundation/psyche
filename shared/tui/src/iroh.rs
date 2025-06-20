use anyhow::Result;
use opentelemetry::metrics::{MetricsError, Result as MetricsResult};
use opentelemetry_sdk::metrics::{
    data::{self, Gauge, Metric, ResourceMetrics, ScopeMetrics, Sum, Temporality},
    producer::MetricProducer,
    InstrumentKind,
};
use opentelemetry_sdk::{InstrumentationScope, Resource};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Import the Iroh types
use iroh_metrics::{parse_prometheus_metrics, MetricsSource};

/// A MetricProducer that reads from Iroh metrics registry on-demand
#[derive(Debug)]
pub struct IrohMetricProducer {
    registry: Arc<dyn MetricsSource>,
    instrumentation_scope: InstrumentationScope,
    // Cache to track counter values for delta calculation
    counter_cache: Arc<Mutex<HashMap<String, u64>>>,
}

impl IrohMetricProducer {
    pub fn new(registry: Arc<dyn MetricsSource>) -> Self {
        Self {
            registry,
            instrumentation_scope: InstrumentationScope::builder("iroh-metrics")
                .with_version("0.1.0")
                .with_schema_url("https://github.com/n0-computer/iroh")
                .build(),
            counter_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn collect_iroh_metrics(&self) -> Result<Vec<Metric>, MetricsError> {
        // Get metrics from Iroh registry
        let metrics_text = self
            .registry
            .encode_openmetrics_to_string()
            .map_err(|e| MetricsError::Other(format!("Failed to encode Iroh metrics: {}", e)))?;

        let parsed_metrics = parse_prometheus_metrics(&metrics_text);
        let mut otel_metrics = Vec::new();
        let now = std::time::SystemTime::now();

        // Lock the counter cache
        let mut counter_cache = self
            .counter_cache
            .lock()
            .map_err(|e| MetricsError::Other(format!("Failed to lock counter cache: {}", e)))?;

        for (metric_name, current_value_f64) in parsed_metrics {
            if metric_name.ends_with("_total") {
                // This is a counter
                let base_name = metric_name.strip_suffix("_total").unwrap();
                let current_value = current_value_f64 as u64;

                // For counters, OpenTelemetry expects cumulative values
                let data_point = data::DataPoint {
                    attributes: vec![].into(),
                    start_time: Some(now),
                    time: Some(now),
                    value: current_value,
                    exemplars: vec![],
                };

                let sum = Sum {
                    data_points: vec![data_point],
                    temporality: Temporality::Cumulative,
                    is_monotonic: true,
                };

                let metric = Metric {
                    name: base_name.into(),
                    description: format!("Iroh counter metric: {}", base_name).into(),
                    unit: "1".into(),
                    data: Box::new(sum),
                };

                otel_metrics.push(metric);

                // Update cache for next collection
                counter_cache.insert(base_name.to_string(), current_value);
            } else {
                // This is a gauge
                let current_value = current_value_f64 as i64;

                let data_point = data::DataPoint {
                    attributes: vec![].into(),
                    start_time: Some(now),
                    time: Some(now),
                    value: current_value,
                    exemplars: vec![],
                };

                let gauge = Gauge {
                    data_points: vec![data_point],
                };

                let metric = Metric {
                    name: metric_name.clone().into(),
                    description: format!("Iroh gauge metric: {}", metric_name).into(),
                    unit: "1".into(),
                    data: Box::new(gauge),
                };

                otel_metrics.push(metric);
            }
        }

        Ok(otel_metrics)
    }
}

impl MetricProducer for IrohMetricProducer {
    fn produce(&self) -> MetricsResult<ResourceMetrics> {
        let metrics = self.collect_iroh_metrics()?;

        let scope_metrics = ScopeMetrics {
            scope: self.instrumentation_scope.clone(),
            metrics,
        };

        Ok(ResourceMetrics {
            resource: Resource::default(), // Will be merged with the main resource
            scope_metrics: vec![scope_metrics],
        })
    }
}

/// Enhanced version that handles labels from the enhanced parser
pub struct IrohMetricProducerWithLabels {
    registry: Arc<dyn MetricsSource>,
    instrumentation_scope: InstrumentationScope,
    counter_cache: Arc<Mutex<HashMap<String, u64>>>,
}

impl IrohMetricProducerWithLabels {
    pub fn new(registry: Arc<dyn MetricsSource>) -> Self {
        Self {
            registry,
            instrumentation_scope: InstrumentationScope::builder("iroh-metrics")
                .with_version("0.1.0")
                .with_schema_url("https://github.com/n0-computer/iroh")
                .build(),
            counter_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn collect_iroh_metrics_with_labels(&self) -> Result<Vec<Metric>, MetricsError> {
        let metrics_text = self
            .registry
            .encode_openmetrics_to_string()
            .map_err(|e| MetricsError::Other(format!("Failed to encode Iroh metrics: {}", e)))?;

        let parsed_metrics = parse_prometheus_metrics_enhanced(&metrics_text);
        let mut otel_metrics_map: HashMap<String, Vec<data::DataPoint<_>>> = HashMap::new();
        let now = std::time::SystemTime::now();

        let mut counter_cache = self
            .counter_cache
            .lock()
            .map_err(|e| MetricsError::Other(format!("Failed to lock counter cache: {}", e)))?;

        for (metric_name, (current_value_f64, labels)) in parsed_metrics {
            // Convert labels to OpenTelemetry attributes
            let attributes: Vec<opentelemetry::KeyValue> = labels
                .into_iter()
                .map(|(k, v)| opentelemetry::KeyValue::new(k, v))
                .collect();

            let (base_name, is_counter) = if metric_name.ends_with("_total") {
                (metric_name.strip_suffix("_total").unwrap(), true)
            } else {
                (metric_name.as_str(), false)
            };

            if is_counter {
                let current_value = current_value_f64 as u64;
                let data_point = data::DataPoint {
                    attributes: attributes.into(),
                    start_time: Some(now),
                    time: Some(now),
                    value: current_value,
                    exemplars: vec![],
                };

                otel_metrics_map
                    .entry(format!("{}_counter", base_name))
                    .or_insert_with(Vec::new)
                    .push(data_point);

                counter_cache.insert(base_name.to_string(), current_value);
            } else {
                let current_value = current_value_f64 as i64;
                let data_point = data::DataPoint {
                    attributes: attributes.into(),
                    start_time: Some(now),
                    time: Some(now),
                    value: current_value,
                    exemplars: vec![],
                };

                otel_metrics_map
                    .entry(format!("{}_gauge", &metric_name))
                    .or_insert_with(Vec::new)
                    .push(data_point);
            }
        }

        // Convert to actual Metric structs
        let mut otel_metrics = Vec::new();
        for (key, data_points) in otel_metrics_map {
            if key.ends_with("_counter") {
                let name = key.strip_suffix("_counter").unwrap();
                let sum = Sum {
                    data_points,
                    temporality: Temporality::Cumulative,
                    is_monotonic: true,
                };

                let metric = Metric {
                    name: name.into(),
                    description: format!("Iroh counter metric: {}", name).into(),
                    unit: "1".into(),
                    data: Box::new(sum),
                };
                otel_metrics.push(metric);
            } else if key.ends_with("_gauge") {
                let name = key.strip_suffix("_gauge").unwrap();
                let gauge = Gauge { data_points };

                let metric = Metric {
                    name: name.into(),
                    description: format!("Iroh gauge metric: {}", name).into(),
                    unit: "1".into(),
                    data: Box::new(gauge),
                };
                otel_metrics.push(metric);
            }
        }

        Ok(otel_metrics)
    }
}

impl MetricProducer for IrohMetricProducerWithLabels {
    fn produce(&self) -> MetricsResult<ResourceMetrics> {
        let metrics = self.collect_iroh_metrics_with_labels()?;

        let scope_metrics = ScopeMetrics {
            scope: self.instrumentation_scope.clone(),
            metrics,
        };

        Ok(ResourceMetrics {
            resource: Resource::default(),
            scope_metrics: vec![scope_metrics],
        })
    }
}

// Enhanced parsing function (from previous implementation)
pub fn parse_prometheus_metrics_enhanced(
    data: &str,
) -> HashMap<String, (f64, HashMap<String, String>)> {
    let mut metrics = HashMap::new();

    for line in data.lines() {
        if line.starts_with('#') || line.trim().is_empty() || line.trim() == "# EOF" {
            continue;
        }

        if let Some((name_and_labels, value_str)) = line.split_once(' ') {
            if let Ok(value) = value_str.trim().parse::<f64>() {
                let mut labels = HashMap::new();
                let metric_name = if let Some((name, labels_str)) = name_and_labels.split_once('{')
                {
                    let labels_str = labels_str.trim_end_matches('}');

                    for label_pair in labels_str.split(',') {
                        if let Some((key, value)) = label_pair.split_once('=') {
                            let key = key.trim();
                            let value = value.trim().trim_matches('"');
                            labels.insert(key.to_string(), value.to_string());
                        }
                    }
                    name.to_string()
                } else {
                    name_and_labels.to_string()
                };

                metrics.insert(metric_name, (value, labels));
            }
        }
    }

    metrics
}
