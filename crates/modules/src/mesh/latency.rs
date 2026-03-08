/*!
 * Latency Tracker
 *
 * WHY THIS EXISTS:
 * ================
 * The LatencyTracker continuously measures response times from all endpoints
 * in the service mesh. This enables smart routing based on actual performance.
 *
 * KEY INSIGHT:
 * We measure latency from OUR gateway to the endpoints, which reflects the
 * actual experience our tenants will have. This is different from endpoint
 * self-reported metrics.
 *
 * WHY WE CAN DO THIS:
 * These are services WE deployed in OUR infrastructure. We have network
 * visibility and can probe them continuously. For external APIs, we can't
 * do this reliably.
 */

use super::registry::{HealthCheckConfig, LatencyStats, ServiceRegistry};
use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;

//═══════════════════════════════════════════════════════════════════════════
// LATENCY TRACKER
//═══════════════════════════════════════════════════════════════════════════

/// Tracks latency for all endpoints in the service mesh
///
/// WHY: For lowest-latency routing, we need real-time measurements of
/// how fast each endpoint responds.
pub struct LatencyTracker {
    registry: Arc<ServiceRegistry>,
    client: reqwest::Client,
}

/// Rolling window for tracking latency percentiles
///
/// WHY: We want to track p50, p95, p99 latencies over a rolling window,
/// not just the last measurement. This gives us a more stable view of
/// endpoint performance.
struct LatencyWindow {
    /// Recent measurements (circular buffer)
    measurements: Vec<u64>,

    /// Current position in buffer
    position: usize,

    /// Maximum measurements to keep
    capacity: usize,
}

impl LatencyWindow {
    fn new(capacity: usize) -> Self {
        Self {
            measurements: Vec::with_capacity(capacity),
            position: 0,
            capacity,
        }
    }

    /// Add a new latency measurement
    fn record(&mut self, latency_ms: u64) {
        if self.measurements.len() < self.capacity {
            self.measurements.push(latency_ms);
        } else {
            self.measurements[self.position] = latency_ms;
            self.position = (self.position + 1) % self.capacity;
        }
    }

    /// Calculate latency statistics from measurements
    fn calculate_stats(&self) -> LatencyStats {
        if self.measurements.is_empty() {
            return LatencyStats::default();
        }

        let mut sorted = self.measurements.clone();
        sorted.sort_unstable();

        let p50_idx = (sorted.len() as f64 * 0.50) as usize;
        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p99_idx = (sorted.len() as f64 * 0.99) as usize;

        LatencyStats {
            p50_ms: sorted.get(p50_idx).copied().unwrap_or(0),
            p95_ms: sorted.get(p95_idx).copied().unwrap_or(0),
            p99_ms: sorted.get(p99_idx).copied().unwrap_or(0),
            last_ms: *sorted.last().unwrap_or(&0),
            updated_at: Utc::now(),
        }
    }
}

impl LatencyTracker {
    /// Create a new latency tracker
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client for latency tracker");

        Self { registry, client }
    }

    /// Start the latency tracker background task
    ///
    /// WHY: Latency tracking must run continuously in the background,
    /// probing endpoints and updating the registry with fresh measurements.
    ///
    /// This spawns a tokio task that:
    /// 1. Lists all services
    /// 2. Probes each endpoint
    /// 3. Calculates latency statistics
    /// 4. Updates registry
    /// 5. Repeats on interval
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Starting latency tracker background task");

            // Probe latency every 10 seconds
            let mut tick = interval(Duration::from_secs(10));

            // Keep rolling windows for each endpoint
            let mut windows = std::collections::HashMap::new();

            loop {
                tick.tick().await;

                if let Err(e) = self.probe_all_services(&mut windows).await {
                    tracing::error!(error = %e, "Latency probe cycle failed");
                }
            }
        })
    }

    /// Probe latency of all services and their endpoints
    async fn probe_all_services(
        &self,
        windows: &mut std::collections::HashMap<String, LatencyWindow>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let services = self.registry.list_services().await;

        tracing::debug!(
            service_count = services.len(),
            "Starting latency probe cycle"
        );

        for service in services {
            for endpoint in &service.endpoints {
                // Probe the endpoint
                if let Some(latency_ms) = self
                    .probe_endpoint(endpoint.url.clone(), &service.config.health_check)
                    .await
                {
                    // Get or create latency window for this endpoint
                    let window = windows
                        .entry(endpoint.id.clone())
                        .or_insert_with(|| LatencyWindow::new(100)); // Keep last 100 measurements

                    // Record measurement
                    window.record(latency_ms);

                    // Calculate stats
                    let stats = window.calculate_stats();

                    // Update registry
                    if let Err(e) = self
                        .registry
                        .update_endpoint_latency(&service.id, &endpoint.id, stats)
                        .await
                    {
                        tracing::error!(
                            service_id = %service.id,
                            endpoint_id = %endpoint.id,
                            error = %e,
                            "Failed to update endpoint latency"
                        );
                    }

                    tracing::trace!(
                        service_id = %service.id,
                        endpoint_id = %endpoint.id,
                        endpoint_url = %endpoint.url,
                        latency_ms = latency_ms,
                        p50_ms = stats.p50_ms,
                        p95_ms = stats.p95_ms,
                        p99_ms = stats.p99_ms,
                        "Updated endpoint latency stats"
                    );
                }
            }
        }

        Ok(())
    }

    /// Probe latency of a single endpoint
    ///
    /// WHY: We hit the health endpoint (or a lightweight ping endpoint)
    /// and measure the round-trip time. This tells us how fast this
    /// endpoint is from our gateway's perspective.
    async fn probe_endpoint(&self, base_url: String, config: &HealthCheckConfig) -> Option<u64> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), config.path);
        let timeout = Duration::from_millis(config.timeout_ms);

        let start = Instant::now();

        match tokio::time::timeout(timeout, self.client.get(&url).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                let latency = start.elapsed();
                Some(latency.as_millis() as u64)
            }
            _ => {
                // If probe fails, don't record latency
                // Health checker will mark it as unhealthy
                None
            }
        }
    }

    /// Manually probe a specific endpoint
    ///
    /// WHY: Sometimes we want to force a latency measurement immediately
    /// (e.g., after deploying to a new region).
    pub async fn probe_endpoint_now(
        &self,
        service_id: &str,
        endpoint_id: &str,
    ) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let service = self
            .registry
            .get_service(service_id)
            .await
            .ok_or_else(|| format!("Service '{}' not found", service_id))?;

        let endpoint = service
            .endpoints
            .iter()
            .find(|e| e.id == endpoint_id)
            .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_id))?;

        let latency_ms = self
            .probe_endpoint(endpoint.url.clone(), &service.config.health_check)
            .await;

        if let Some(ms) = latency_ms {
            // Create a simple stats with just this measurement
            let stats = LatencyStats {
                p50_ms: ms,
                p95_ms: ms,
                p99_ms: ms,
                last_ms: ms,
                updated_at: Utc::now(),
            };

            self.registry
                .update_endpoint_latency(service_id, endpoint_id, stats)
                .await?;
        }

        Ok(latency_ms)
    }

    /// Get latency statistics for all endpoints of a service
    ///
    /// WHY: Useful for debugging and monitoring. Which endpoints are fastest?
    pub async fn get_service_latency_stats(
        &self,
        service_id: &str,
    ) -> Option<Vec<EndpointLatencyInfo>> {
        let service = self.registry.get_service(service_id).await?;

        Some(
            service
                .endpoints
                .iter()
                .map(|e| EndpointLatencyInfo {
                    endpoint_id: e.id.clone(),
                    url: e.url.clone(),
                    region: e.region.clone(),
                    stats: e.latency,
                })
                .collect(),
        )
    }
}

//═══════════════════════════════════════════════════════════════════════════
// LATENCY INFORMATION
//═══════════════════════════════════════════════════════════════════════════

/// Latency information for an endpoint
#[derive(Debug, Clone, serde::Serialize)]
pub struct EndpointLatencyInfo {
    pub endpoint_id: String,
    pub url: String,
    pub region: String,
    pub stats: LatencyStats,
}

impl EndpointLatencyInfo {
    /// Is this endpoint performing well?
    pub fn is_fast(&self) -> bool {
        self.stats.p95_ms < 100 // < 100ms is considered fast
    }

    /// Is this endpoint performing poorly?
    pub fn is_slow(&self) -> bool {
        self.stats.p95_ms > 500 // > 500ms is considered slow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_window() {
        let mut window = LatencyWindow::new(5);

        // Add measurements
        window.record(10);
        window.record(20);
        window.record(30);
        window.record(40);
        window.record(50);

        let stats = window.calculate_stats();
        assert_eq!(stats.p50_ms, 30); // median
        assert_eq!(stats.last_ms, 50); // last measurement

        // Add more (wraps around)
        window.record(60);
        let stats = window.calculate_stats();
        assert_eq!(stats.p50_ms, 40); // [20, 30, 40, 50, 60]
    }

    #[test]
    fn test_latency_window_percentiles() {
        let mut window = LatencyWindow::new(100);

        // Add 100 measurements: 1ms to 100ms
        for i in 1..=100 {
            window.record(i);
        }

        let stats = window.calculate_stats();
        assert_eq!(stats.p50_ms, 50); // 50th percentile
        assert_eq!(stats.p95_ms, 95); // 95th percentile
        assert_eq!(stats.p99_ms, 99); // 99th percentile
    }
}
