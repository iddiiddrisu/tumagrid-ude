/*!
 * Service Mesh Router
 *
 * WHY THIS EXISTS:
 * ================
 * The ServiceMeshRouter is the brain of the mesh. It decides which endpoint
 * to route each request to, based on:
 * - Health status (avoid unhealthy endpoints)
 * - Latency (prefer fastest endpoints)
 * - Region affinity (prefer nearby endpoints)
 * - Load balancing (distribute load)
 *
 * CRITICAL: This is ONLY for services we deployed and control.
 * External APIs use the RestApiExecutor which has no smart routing.
 */

use super::registry::{ManagedEndpoint, ServiceRegistry};
use serde::Serialize;
use ude_core::RoutingStrategy; // Use the core definition
use std::sync::Arc;

//═══════════════════════════════════════════════════════════════════════════
// SERVICE MESH ROUTER
//═══════════════════════════════════════════════════════════════════════════

/// Router that selects endpoints using smart routing strategies
pub struct ServiceMeshRouter {
    registry: Arc<ServiceRegistry>,

    /// Round-robin state (per service)
    rr_state: Arc<tokio::sync::RwLock<std::collections::HashMap<String, usize>>>,
}

impl ServiceMeshRouter {
    /// Create a new service mesh router
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        Self {
            registry,
            rr_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Route a request to the best endpoint for a service
    ///
    /// WHY: This is the core routing logic. Given a service ID and strategy,
    /// pick the best endpoint to send the request to.
    ///
    /// Returns:
    /// - Ok(endpoint) if a suitable endpoint is found
    /// - Err if service doesn't exist or no healthy endpoints
    pub async fn route(
        &self,
        service_id: &str,
        strategy: &RoutingStrategy,
    ) -> Result<ManagedEndpoint, RouteError> {
        // Get healthy endpoints for this service
        let endpoints = self.registry.get_healthy_endpoints(service_id).await;

        if endpoints.is_empty() {
            return Err(RouteError::NoHealthyEndpoints(service_id.to_string()));
        }

        // Apply routing strategy
        let selected = match strategy {
            RoutingStrategy::LowestLatency => self.route_lowest_latency(&endpoints),

            RoutingStrategy::RegionAffinity { preferred_region } => {
                self.route_region_affinity(&endpoints, preferred_region)
            }

            RoutingStrategy::RoundRobin => self.route_round_robin(service_id, &endpoints).await,

            RoutingStrategy::Weighted => self.route_weighted(&endpoints),

            RoutingStrategy::Random => self.route_random(&endpoints),
        };

        selected.ok_or_else(|| RouteError::NoSuitableEndpoint(service_id.to_string()))
    }

    /// Route to endpoint with lowest p50 latency
    ///
    /// WHY: For most requests, we want the fastest endpoint. Use p50 (median)
    /// instead of p99 to avoid being too sensitive to outliers.
    fn route_lowest_latency(&self, endpoints: &[ManagedEndpoint]) -> Option<ManagedEndpoint> {
        endpoints.iter().min_by_key(|e| e.latency.p50_ms).cloned()
    }

    /// Route to preferred region, fall back to lowest latency
    ///
    /// WHY: For data locality or compliance, try to use an endpoint in
    /// the preferred region. If none available or unhealthy, fall back.
    fn route_region_affinity(
        &self,
        endpoints: &[ManagedEndpoint],
        preferred_region: &str,
    ) -> Option<ManagedEndpoint> {
        // Try to find an endpoint in preferred region
        let in_region: Vec<_> = endpoints
            .iter()
            .filter(|e| e.region == preferred_region)
            .collect();

        if !in_region.is_empty() {
            // Multiple endpoints in region? Pick lowest latency
            return in_region
                .iter()
                .min_by_key(|e| e.latency.p50_ms)
                .cloned()
                .cloned();
        }

        // No endpoints in preferred region, fall back to lowest latency
        tracing::debug!(
            preferred_region = %preferred_region,
            "No endpoints in preferred region, falling back to lowest latency"
        );

        self.route_lowest_latency(endpoints)
    }

    /// Route using round-robin
    ///
    /// WHY: For background jobs or bulk operations, distribute load evenly
    /// across all healthy endpoints.
    async fn route_round_robin(
        &self,
        service_id: &str,
        endpoints: &[ManagedEndpoint],
    ) -> Option<ManagedEndpoint> {
        if endpoints.is_empty() {
            return None;
        }

        let mut state = self.rr_state.write().await;
        let counter = state.entry(service_id.to_string()).or_insert(0);

        let idx = *counter % endpoints.len();
        *counter = (*counter + 1) % endpoints.len();

        endpoints.get(idx).cloned()
    }

    /// Route using weighted distribution
    ///
    /// WHY: For canary deployments or gradual rollouts. New endpoints get
    /// small weight, increase as confidence grows.
    fn route_weighted(&self, endpoints: &[ManagedEndpoint]) -> Option<ManagedEndpoint> {
        if endpoints.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: f64 = endpoints.iter().map(|e| e.weight).sum();

        if total_weight == 0.0 {
            // All weights are zero? Fall back to round-robin behavior
            return endpoints.first().cloned();
        }

        // Generate random number between 0 and total_weight
        let mut rng = rand::thread_rng();
        use rand::Rng;
        let mut target = rng.gen_range(0.0..total_weight);

        // Select endpoint based on weight
        for endpoint in endpoints {
            target -= endpoint.weight;
            if target <= 0.0 {
                return Some(endpoint.clone());
            }
        }

        // Fallback (shouldn't happen due to math, but just in case)
        endpoints.last().cloned()
    }

    /// Route randomly (for testing)
    fn route_random(&self, endpoints: &[ManagedEndpoint]) -> Option<ManagedEndpoint> {
        if endpoints.is_empty() {
            return None;
        }

        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        endpoints.choose(&mut rng).cloned()
    }

    /// Get routing statistics for a service
    ///
    /// WHY: Useful for debugging and monitoring. Which endpoints are
    /// being used most? Are we load balancing effectively?
    pub async fn get_routing_stats(&self, service_id: &str) -> Option<RoutingStats> {
        let service = self.registry.get_service(service_id).await?;

        let healthy_endpoints = self.registry.get_healthy_endpoints(service_id).await;

        let fastest = healthy_endpoints.iter().min_by_key(|e| e.latency.p50_ms);

        let slowest = healthy_endpoints.iter().max_by_key(|e| e.latency.p50_ms);

        Some(RoutingStats {
            total_endpoints: service.endpoints.len(),
            healthy_endpoints: healthy_endpoints.len(),
            fastest_endpoint: fastest.map(|e| EndpointSummary {
                id: e.id.clone(),
                url: e.url.clone(),
                region: e.region.clone(),
                latency_p50_ms: e.latency.p50_ms,
            }),
            slowest_endpoint: slowest.map(|e| EndpointSummary {
                id: e.id.clone(),
                url: e.url.clone(),
                region: e.region.clone(),
                latency_p50_ms: e.latency.p50_ms,
            }),
        })
    }
}

//═══════════════════════════════════════════════════════════════════════════
// ROUTING STATISTICS
//═══════════════════════════════════════════════════════════════════════════

/// Statistics about routing for a service
#[derive(Debug, Clone, Serialize)]
pub struct RoutingStats {
    /// Total number of endpoints registered
    pub total_endpoints: usize,

    /// Number of healthy endpoints available for routing
    pub healthy_endpoints: usize,

    /// Fastest endpoint (lowest p50)
    pub fastest_endpoint: Option<EndpointSummary>,

    /// Slowest endpoint (highest p50)
    pub slowest_endpoint: Option<EndpointSummary>,
}

/// Summary information about an endpoint
#[derive(Debug, Clone, Serialize)]
pub struct EndpointSummary {
    pub id: String,
    pub url: String,
    pub region: String,
    pub latency_p50_ms: u64,
}

//═══════════════════════════════════════════════════════════════════════════
// ERROR TYPES
//═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Service '{0}' has no healthy endpoints")]
    NoHealthyEndpoints(String),

    #[error("Service '{0}' has no suitable endpoint for the requested strategy")]
    NoSuitableEndpoint(String),

    #[error("Service '{0}' not found in registry")]
    ServiceNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::registry::*;

    fn create_test_endpoint(id: &str, region: &str, latency_ms: u64) -> ManagedEndpoint {
        ManagedEndpoint {
            id: id.to_string(),
            service_id: "test-service".to_string(),
            url: format!("https://{}.example.com", id),
            region: region.to_string(),
            health: HealthStatus::Healthy,
            latency: LatencyStats {
                p50_ms: latency_ms,
                p95_ms: latency_ms * 2,
                p99_ms: latency_ms * 3,
                last_ms: latency_ms,
                updated_at: chrono::Utc::now(),
            },
            node_id: None,
            deployed_at: chrono::Utc::now(),
            weight: 1.0,
        }
    }

    #[test]
    fn test_route_lowest_latency() {
        let router = ServiceMeshRouter::new(Arc::new(ServiceRegistry::new()));

        let endpoints = vec![
            create_test_endpoint("ep1", "us-east", 50),
            create_test_endpoint("ep2", "us-west", 20), // fastest
            create_test_endpoint("ep3", "eu-west", 100),
        ];

        let selected = router.route_lowest_latency(&endpoints).unwrap();
        assert_eq!(selected.id, "ep2");
        assert_eq!(selected.latency.p50_ms, 20);
    }

    #[test]
    fn test_route_region_affinity() {
        let router = ServiceMeshRouter::new(Arc::new(ServiceRegistry::new()));

        let endpoints = vec![
            create_test_endpoint("ep1", "us-east", 50),
            create_test_endpoint("ep2", "eu-west", 20),
            create_test_endpoint("ep3", "eu-west", 30),
        ];

        // Prefer eu-west region
        let selected = router.route_region_affinity(&endpoints, "eu-west").unwrap();
        assert_eq!(selected.region, "eu-west");
        assert_eq!(selected.id, "ep2"); // lowest latency in eu-west

        // Prefer region that doesn't exist - falls back to lowest overall
        let selected = router
            .route_region_affinity(&endpoints, "ap-south")
            .unwrap();
        assert_eq!(selected.id, "ep2"); // overall lowest
    }

    #[test]
    fn test_route_weighted() {
        let router = ServiceMeshRouter::new(Arc::new(ServiceRegistry::new()));

        let mut endpoints = vec![
            create_test_endpoint("ep1", "us-east", 50),
            create_test_endpoint("ep2", "us-west", 20),
        ];

        // Set weights: ep1 = 90%, ep2 = 10% (canary)
        endpoints[0].weight = 9.0;
        endpoints[1].weight = 1.0;

        // Run many times, check distribution
        let mut counts = std::collections::HashMap::new();
        for _ in 0..1000 {
            let selected = router.route_weighted(&endpoints).unwrap();
            *counts.entry(selected.id).or_insert(0) += 1;
        }

        // Should be roughly 900/100 split (with some randomness)
        let ep1_count = counts.get("ep1").unwrap_or(&0);
        let ep2_count = counts.get("ep2").unwrap_or(&0);

        assert!(*ep1_count > 800); // At least 80% to ep1
        assert!(*ep2_count < 200); // At most 20% to ep2
    }
}
