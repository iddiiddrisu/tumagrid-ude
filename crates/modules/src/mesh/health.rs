/*!
 * Health Checker
 *
 * WHY THIS EXISTS:
 * ================
 * The HealthChecker continuously monitors all endpoints in the service mesh
 * to determine which ones are healthy and should receive traffic.
 *
 * This is critical for:
 * - Automatic failover when endpoints go down
 * - Avoiding routing traffic to broken services
 * - Fast detection of infrastructure issues
 *
 * WHY WE CAN DO THIS:
 * These are services WE deployed and control. We trust their health endpoints.
 * For external APIs (RestApi executor), we can't rely on health checks.
 */

use super::registry::{HealthCheckConfig, HealthStatus, ServiceRegistry};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

//═══════════════════════════════════════════════════════════════════════════
// HEALTH CHECKER
//═══════════════════════════════════════════════════════════════════════════

/// Monitors health of all endpoints in the service mesh
///
/// WHY: We need continuous health monitoring to:
/// - Detect when endpoints go down
/// - Automatically route around failures
/// - Track service availability over time
pub struct HealthChecker {
    registry: Arc<ServiceRegistry>,
    client: reqwest::Client,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client for health checker");

        Self { registry, client }
    }

    /// Start the health checker background task
    ///
    /// WHY: Health checking must run continuously in the background,
    /// updating the registry as endpoint health changes.
    ///
    /// This spawns a tokio task that:
    /// 1. Lists all services
    /// 2. Checks each endpoint's health
    /// 3. Updates registry with new health status
    /// 4. Repeats on interval
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Starting health checker background task");

            // Check health every 10 seconds by default
            // Individual services can override via their health_check config
            let mut tick = interval(Duration::from_secs(10));

            loop {
                tick.tick().await;

                if let Err(e) = self.check_all_services().await {
                    tracing::error!(error = %e, "Health check cycle failed");
                }
            }
        })
    }

    /// Check health of all services and their endpoints
    async fn check_all_services(&self) -> Result<(), Box<dyn std::error::Error>> {
        let services = self.registry.list_services().await;

        tracing::debug!(
            service_count = services.len(),
            "Starting health check cycle"
        );

        for service in services {
            for endpoint in &service.endpoints {
                let health = self
                    .check_endpoint(endpoint.url.clone(), &service.config.health_check)
                    .await;

                // Update registry with new health status
                if let Err(e) = self
                    .registry
                    .update_endpoint_health(&service.id, &endpoint.id, health)
                    .await
                {
                    tracing::error!(
                        service_id = %service.id,
                        endpoint_id = %endpoint.id,
                        error = %e,
                        "Failed to update endpoint health"
                    );
                }

                // Log health status changes
                if health != endpoint.health {
                    tracing::info!(
                        service_id = %service.id,
                        endpoint_id = %endpoint.id,
                        endpoint_url = %endpoint.url,
                        old_status = ?endpoint.health,
                        new_status = ?health,
                        "Endpoint health status changed"
                    );
                }
            }
        }

        Ok(())
    }

    /// Check health of a single endpoint
    ///
    /// WHY: Each service has a health endpoint (typically /health).
    /// We hit that endpoint and determine if the service is healthy.
    async fn check_endpoint(&self, base_url: String, config: &HealthCheckConfig) -> HealthStatus {
        let url = format!("{}{}", base_url.trim_end_matches('/'), config.path);
        let timeout = Duration::from_millis(config.timeout_ms);

        tracing::trace!(url = %url, "Checking endpoint health");

        // Send health check request
        match tokio::time::timeout(timeout, self.client.get(&url).send()).await {
            // Request succeeded within timeout
            Ok(Ok(response)) => {
                let status_code = response.status().as_u16();

                if config.expected_status.contains(&status_code) {
                    tracing::trace!(
                        url = %url,
                        status = status_code,
                        "Endpoint is healthy"
                    );
                    HealthStatus::Healthy
                } else if status_code >= 500 {
                    tracing::warn!(
                        url = %url,
                        status = status_code,
                        "Endpoint returned server error"
                    );
                    HealthStatus::Unhealthy
                } else {
                    // 4xx errors might mean the health endpoint doesn't exist
                    // but the service might still be running
                    tracing::warn!(
                        url = %url,
                        status = status_code,
                        "Endpoint returned unexpected status"
                    );
                    HealthStatus::Degraded
                }
            }

            // Request failed
            Ok(Err(e)) => {
                tracing::warn!(
                    url = %url,
                    error = %e,
                    "Health check request failed"
                );
                HealthStatus::Unhealthy
            }

            // Timeout
            Err(_) => {
                tracing::warn!(
                    url = %url,
                    timeout_ms = config.timeout_ms,
                    "Health check timed out"
                );
                HealthStatus::Unhealthy
            }
        }
    }

    /// Manually check a specific endpoint
    ///
    /// WHY: Sometimes we want to force a health check immediately
    /// (e.g., after deploying a new version).
    pub async fn check_endpoint_now(
        &self,
        service_id: &str,
        endpoint_id: &str,
    ) -> Result<HealthStatus, Box<dyn std::error::Error>> {
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

        let health = self
            .check_endpoint(endpoint.url.clone(), &service.config.health_check)
            .await;

        self.registry
            .update_endpoint_health(service_id, endpoint_id, health)
            .await?;

        Ok(health)
    }

    /// Get health statistics for a service
    ///
    /// WHY: Useful for monitoring and alerting. How many endpoints are healthy?
    pub async fn get_service_health_stats(&self, service_id: &str) -> Option<HealthStats> {
        let service = self.registry.get_service(service_id).await?;

        let total = service.endpoints.len();
        let healthy = service
            .endpoints
            .iter()
            .filter(|e| matches!(e.health, HealthStatus::Healthy))
            .count();
        let degraded = service
            .endpoints
            .iter()
            .filter(|e| matches!(e.health, HealthStatus::Degraded))
            .count();
        let unhealthy = service
            .endpoints
            .iter()
            .filter(|e| matches!(e.health, HealthStatus::Unhealthy))
            .count();
        let unknown = service
            .endpoints
            .iter()
            .filter(|e| matches!(e.health, HealthStatus::Unknown))
            .count();

        Some(HealthStats {
            total,
            healthy,
            degraded,
            unhealthy,
            unknown,
            availability_percent: (healthy as f64 / total as f64) * 100.0,
        })
    }
}

//═══════════════════════════════════════════════════════════════════════════
// HEALTH STATISTICS
//═══════════════════════════════════════════════════════════════════════════

/// Health statistics for a service
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStats {
    /// Total number of endpoints
    pub total: usize,

    /// Number of healthy endpoints
    pub healthy: usize,

    /// Number of degraded endpoints
    pub degraded: usize,

    /// Number of unhealthy endpoints
    pub unhealthy: usize,

    /// Number of endpoints with unknown health
    pub unknown: usize,

    /// Percentage of healthy endpoints
    pub availability_percent: f64,
}

impl HealthStats {
    /// Is this service in a good state?
    pub fn is_healthy(&self) -> bool {
        self.healthy > 0 && self.unhealthy == 0
    }

    /// Is this service at risk?
    pub fn is_at_risk(&self) -> bool {
        self.healthy > 0 && self.unhealthy > 0
    }

    /// Is this service completely down?
    pub fn is_down(&self) -> bool {
        self.healthy == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_stats() {
        let stats = HealthStats {
            total: 3,
            healthy: 3,
            degraded: 0,
            unhealthy: 0,
            unknown: 0,
            availability_percent: 100.0,
        };

        assert!(stats.is_healthy());
        assert!(!stats.is_at_risk());
        assert!(!stats.is_down());

        let stats = HealthStats {
            total: 3,
            healthy: 2,
            degraded: 0,
            unhealthy: 1,
            unknown: 0,
            availability_percent: 66.7,
        };

        assert!(!stats.is_healthy());
        assert!(stats.is_at_risk());
        assert!(!stats.is_down());

        let stats = HealthStats {
            total: 3,
            healthy: 0,
            degraded: 0,
            unhealthy: 3,
            unknown: 0,
            availability_percent: 0.0,
        };

        assert!(!stats.is_healthy());
        assert!(!stats.is_at_risk());
        assert!(stats.is_down());
    }
}
