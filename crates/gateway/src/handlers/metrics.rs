use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Handler for Prometheus metrics endpoint
///
/// Returns metrics in Prometheus text format for scraping
pub async fn metrics() -> Response {
    match crate::telemetry::get_prometheus_metrics() {
        Ok(metrics) => (
            StatusCode::OK,
            [("content-type", "text/plain; version=0.0.4")],
            metrics,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to gather metrics: {}", e),
        )
            .into_response(),
    }
}
