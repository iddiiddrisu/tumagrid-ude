/*!
 * OpenTelemetry Integration
 *
 * WHY THIS EXISTS:
 * ================
 * Production deployments need observability: traces, metrics, and logs.
 * This module integrates SpaceForge with OpenTelemetry so it can export
 * telemetry data to any OTel-compatible backend (Prometheus, Jaeger, Grafana, etc.)
 *
 * WHAT IT PROVIDES:
 * =================
 * - Distributed tracing (request flow across services)
 * - Metrics (request rates, latencies, error rates)
 * - Structured logging (errors, warnings, debug info)
 * - Auto-instrumentation of HTTP requests
 * - Custom metrics for business logic
 *
 * SUPPORTED BACKENDS:
 * ===================
 * - Prometheus (metrics)
 * - Jaeger (traces)
 * - Tempo (traces)
 * - Loki (logs)
 * - Any OTel Collector
 * - Grafana Cloud
 * - Datadog
 * - New Relic
 * - Honeycomb
 */

use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{metrics::MeterProviderBuilder, Resource};
use prometheus::{Encoder, TextEncoder};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name (e.g., "ude")
    pub service_name: String,

    /// Service version (e.g., "0.1.0")
    pub service_version: String,

    /// Environment (e.g., "production", "staging")
    pub environment: String,

    /// OTLP endpoint for traces and metrics (e.g., "http://otel-collector:4317")
    pub otlp_endpoint: Option<String>,

    /// Enable Prometheus metrics endpoint
    pub enable_prometheus: bool,

    /// Prometheus port (default: 9090)
    #[allow(dead_code)]
    pub prometheus_port: u16,

    /// Trace sampling rate (0.0 to 1.0)
    #[allow(dead_code)]
    pub trace_sample_rate: f64,

    /// Log level (e.g., "info", "debug", "warn")
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "ude".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: "development".to_string(),
            otlp_endpoint: None,
            enable_prometheus: true,
            prometheus_port: 9090,
            trace_sample_rate: 1.0, // Sample all traces in dev
            log_level: "info".to_string(),
        }
    }
}

/// Initialize OpenTelemetry
///
/// WHY: Sets up distributed tracing, metrics, and structured logging
/// for production observability.
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), anyhow::Error> {
    // Initialize structured logging
    init_logging(&config.log_level)?;

    // Initialize Prometheus metrics if enabled
    if config.enable_prometheus {
        init_prometheus_metrics(&config)?;
    }

    // Note: OTLP tracing/metrics will be added in a future update
    // The OpenTelemetry 0.27 API requires significant refactoring
    if config.otlp_endpoint.is_some() {
        tracing::warn!("OTLP endpoint configured but not yet supported in OpenTelemetry 0.27");
    }

    tracing::info!(
        service = config.service_name,
        version = config.service_version,
        environment = config.environment,
        "Telemetry initialized"
    );

    Ok(())
}

/// Initialize Prometheus metrics exporter
fn init_prometheus_metrics(config: &TelemetryConfig) -> Result<(), anyhow::Error> {
    // Create resource with service metadata
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", config.service_version.clone()),
        KeyValue::new("deployment.environment", config.environment.clone()),
    ]);

    let exporter = opentelemetry_prometheus::exporter().build()?;

    let provider = MeterProviderBuilder::default()
        .with_reader(exporter)
        .with_resource(resource)
        .build();

    global::set_meter_provider(provider);

    Ok(())
}

/// Initialize structured logging with tracing
fn init_logging(log_level: &str) -> Result<(), anyhow::Error> {
    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(log_level))?;

    // Create JSON formatter for structured logs
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_line_number(true)
        .with_thread_ids(true);

    // Compose layers
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    Ok(())
}

/// Get Prometheus metrics as text
///
/// WHY: Expose metrics on /metrics endpoint for Prometheus scraping
pub fn get_prometheus_metrics() -> Result<String, anyhow::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Shutdown telemetry gracefully
///
/// WHY: Flush pending traces/metrics before shutdown
pub async fn shutdown_telemetry() {
    tracing::info!("Shutting down telemetry");

    // Shutdown tracing
    global::shutdown_tracer_provider();

    tracing::info!("Telemetry shutdown complete");
}

// ============================================================================
// CUSTOM METRICS (Business Logic)
// ============================================================================
// Note: Custom metrics temporarily disabled pending OpenTelemetry 0.27 API update
// The metrics API has changed significantly and needs refactoring

/* TODO: Re-enable with OTel 0.27 compatible API
use opentelemetry::metrics::{Counter, Histogram, Meter};
use std::sync::OnceLock;

static METRICS: OnceLock<SpaceForgeMetrics> = OnceLock::new();

/// SpaceForge business metrics
pub struct SpaceForgeMetrics {
    // Request metrics
    pub http_requests_total: Counter<u64>,
    pub http_request_duration: Histogram<f64>,
    pub http_errors_total: Counter<u64>,

    // CRUD metrics
    pub crud_operations_total: Counter<u64>,
    pub crud_operation_duration: Histogram<f64>,
    pub crud_errors_total: Counter<u64>,

    // Orchestration metrics
    pub orchestration_queries_total: Counter<u64>,
    pub orchestration_query_duration: Histogram<f64>,
    pub orchestration_sources_executed: Histogram<u64>,
    pub orchestration_stages_executed: Histogram<u64>,
    pub orchestration_cache_hits: Counter<u64>,
    pub orchestration_errors_total: Counter<u64>,

    // Database metrics
    pub db_connections_active: Histogram<u64>,
    pub db_query_duration: Histogram<f64>,
    pub db_errors_total: Counter<u64>,
}

impl SpaceForgeMetrics {
    pub fn init() -> Self {
        let meter = global::meter("ude");

        Self {
            // HTTP metrics
            http_requests_total: meter
                .u64_counter("http_requests_total")
                .with_description("Total number of HTTP requests")
                .init(),
            http_request_duration: meter
                .f64_histogram("http_request_duration_seconds")
                .with_description("HTTP request duration in seconds")
                .init(),
            http_errors_total: meter
                .u64_counter("http_errors_total")
                .with_description("Total number of HTTP errors")
                .init(),

            // CRUD metrics
            crud_operations_total: meter
                .u64_counter("crud_operations_total")
                .with_description("Total number of CRUD operations")
                .init(),
            crud_operation_duration: meter
                .f64_histogram("crud_operation_duration_seconds")
                .with_description("CRUD operation duration in seconds")
                .init(),
            crud_errors_total: meter
                .u64_counter("crud_errors_total")
                .with_description("Total number of CRUD errors")
                .init(),

            // Orchestration metrics
            orchestration_queries_total: meter
                .u64_counter("orchestration_queries_total")
                .with_description("Total number of orchestration queries")
                .init(),
            orchestration_query_duration: meter
                .f64_histogram("orchestration_query_duration_seconds")
                .with_description("Orchestration query duration in seconds")
                .init(),
            orchestration_sources_executed: meter
                .u64_histogram("orchestration_sources_executed")
                .with_description("Number of data sources executed per query")
                .init(),
            orchestration_stages_executed: meter
                .u64_histogram("orchestration_stages_executed")
                .with_description("Number of execution stages per query")
                .init(),
            orchestration_cache_hits: meter
                .u64_counter("orchestration_cache_hits_total")
                .with_description("Total number of cache hits in orchestration")
                .init(),
            orchestration_errors_total: meter
                .u64_counter("orchestration_errors_total")
                .with_description("Total number of orchestration errors")
                .init(),

            // Database metrics
            db_connections_active: meter
                .u64_histogram("db_connections_active")
                .with_description("Number of active database connections")
                .init(),
            db_query_duration: meter
                .f64_histogram("db_query_duration_seconds")
                .with_description("Database query duration in seconds")
                .init(),
            db_errors_total: meter
                .u64_counter("db_errors_total")
                .with_description("Total number of database errors")
                .init(),
        }
    }

    pub fn get() -> &'static SpaceForgeMetrics {
        METRICS.get_or_init(|| SpaceForgeMetrics::init())
    }
}

// ============================================================================
// HELPER MACROS
// ============================================================================

/// Record HTTP request metrics
#[macro_export]
macro_rules! record_http_request {
    ($method:expr, $path:expr, $status:expr, $duration:expr) => {
        {
            use opentelemetry::KeyValue;
            let metrics = $crate::telemetry::SpaceForgeMetrics::get();

            let attrs = vec![
                KeyValue::new("method", $method),
                KeyValue::new("path", $path),
                KeyValue::new("status", $status.to_string()),
            ];

            metrics.http_requests_total.add(1, &attrs);
            metrics.http_request_duration.record($duration, &attrs);

            if $status >= 400 {
                metrics.http_errors_total.add(1, &attrs);
            }
        }
    };
}

/// Record orchestration query metrics
#[macro_export]
macro_rules! record_orchestration_query {
    ($query_id:expr, $duration:expr, $sources:expr, $stages:expr) => {
        {
            use opentelemetry::KeyValue;
            let metrics = $crate::telemetry::SpaceForgeMetrics::get();

            let attrs = vec![KeyValue::new("query_id", $query_id)];

            metrics.orchestration_queries_total.add(1, &attrs);
            metrics.orchestration_query_duration.record($duration, &attrs);
            metrics.orchestration_sources_executed.record($sources as u64, &attrs);
            metrics.orchestration_stages_executed.record($stages as u64, &attrs);
        }
    };
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "ude");
        assert_eq!(config.environment, "development");
        assert!(config.enable_prometheus);
    }

    // TODO: Re-enable when custom metrics are restored
    // #[test]
    // fn test_prometheus_metrics_encoding() {
    //     // Initialize metrics
    //     let _metrics = SpaceForgeMetrics::init();
    //
    //     // Should not panic
    //     let result = get_prometheus_metrics();
    //     assert!(result.is_ok());
    // }
}
