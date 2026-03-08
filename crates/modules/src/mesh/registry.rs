/*!
 * Service Registry
 *
 * WHY THIS EXISTS:
 * ================
 * The ServiceRegistry manages all services that are deployed and controlled by
 * the SpaceCloud platform (via the runner). These are "first-class" services
 * that are part of the orchestration mesh.
 *
 * CRITICAL DISTINCTION:
 * - ServiceMesh: For services WE deploy and control (multi-region, health reporting)
 * - RestApi: For external APIs and legacy VMs not in our mesh
 *
 * Services in the registry:
 * - Are deployed via SpaceCloud runner
 * - Report health to control plane
 * - Have multi-region endpoints
 * - Get smart routing based on latency
 * - Are part of the orchestration cluster
 */

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

//═══════════════════════════════════════════════════════════════════════════
// CORE TYPES
//═══════════════════════════════════════════════════════════════════════════

/// Unique identifier for a service in the mesh
pub type ServiceId = String;

/// Unique identifier for a service endpoint
pub type EndpointId = String;

/// Registry of all managed services in the mesh
///
/// WHY: We need a central place to track all services we've deployed,
/// their endpoints, health status, and routing information.
pub struct ServiceRegistry {
    /// All registered services
    services: Arc<RwLock<HashMap<ServiceId, ManagedService>>>,
}

/// A service that is deployed and managed by SpaceCloud
///
/// WHY: These are services we have full control over - we deployed them,
/// we monitor them, we can route intelligently to their endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedService {
    /// Unique service identifier (e.g., "hubtel-payments")
    pub id: ServiceId,

    /// Human-readable name
    pub name: String,

    /// Service category for organization
    pub category: ServiceCategory,

    /// All endpoints for this service (multi-region)
    pub endpoints: Vec<ManagedEndpoint>,

    /// How this service was deployed
    pub deployment: DeploymentInfo,

    /// Service-level configuration
    pub config: ServiceConfig,

    /// When this service was registered
    pub registered_at: DateTime<Utc>,
}

/// Categories for organizing services
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceCategory {
    /// Payment processing services
    Payments,

    /// SMS/communication services
    Messaging,

    /// Authentication services
    Auth,

    /// Storage services
    Storage,

    /// Analytics services
    Analytics,

    /// Custom/other services
    Custom(String),
}

/// An endpoint for a managed service
///
/// WHY: Services are deployed to multiple regions for low latency and high
/// availability. Each endpoint represents one regional deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedEndpoint {
    /// Unique endpoint identifier
    pub id: EndpointId,

    /// Service this endpoint belongs to
    pub service_id: ServiceId,

    /// Full URL of this endpoint
    pub url: String,

    /// Region where this endpoint is deployed
    pub region: String,

    /// Health status (updated by health checker)
    #[serde(skip)]
    pub health: HealthStatus,

    /// Latency statistics (updated by latency tracker)
    #[serde(skip)]
    pub latency: LatencyStats,

    /// Which node/container is running this endpoint
    pub node_id: Option<String>,

    /// When this endpoint was deployed
    pub deployed_at: DateTime<Utc>,

    /// Weight for weighted routing (1.0 = default)
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

/// Health status of an endpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Endpoint is healthy and accepting traffic
    Healthy,

    /// Endpoint is degraded but still usable
    Degraded,

    /// Endpoint is unhealthy, should not receive traffic
    Unhealthy,

    /// Health status unknown (newly registered)
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Unknown
    }
}

impl HealthStatus {
    /// Can this endpoint receive traffic?
    pub fn is_available(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
}

/// Latency statistics for an endpoint
///
/// WHY: For smart routing, we need to know which endpoints are fastest
/// from the perspective of our gateway.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LatencyStats {
    /// 50th percentile latency (median)
    pub p50_ms: u64,

    /// 95th percentile latency
    pub p95_ms: u64,

    /// 99th percentile latency
    pub p99_ms: u64,

    /// Last measured latency
    pub last_ms: u64,

    /// When these stats were last updated
    pub updated_at: DateTime<Utc>,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            p50_ms: u64::MAX, // Unknown = worst case
            p95_ms: u64::MAX,
            p99_ms: u64::MAX,
            last_ms: u64::MAX,
            updated_at: Utc::now(),
        }
    }
}

/// Information about how a service was deployed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentInfo {
    /// How was this service deployed?
    pub method: DeploymentMethod,

    /// Runner version that deployed it
    pub runner_version: Option<String>,

    /// Container image (if applicable)
    pub image: Option<String>,

    /// Git commit/tag (if applicable)
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentMethod {
    /// Deployed via SpaceCloud runner (first-class)
    Runner,

    /// Manually registered (partner-hosted on our infra)
    Manual,

    /// External service we're proxying to
    External,
}

/// Service-level configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Default timeout for requests to this service
    pub default_timeout_ms: u64,

    /// Health check configuration
    pub health_check: HealthCheckConfig,

    /// Whether this service requires authentication
    pub requires_auth: bool,

    /// Rate limiting (requests per second)
    pub rate_limit: Option<u32>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30000,
            health_check: HealthCheckConfig::default(),
            requires_auth: true,
            rate_limit: None,
        }
    }
}

/// Health check configuration for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Path to hit for health checks (relative to endpoint URL)
    pub path: String,

    /// How often to check health
    pub interval_secs: u64,

    /// Timeout for health check request
    pub timeout_ms: u64,

    /// Expected HTTP status code(s)
    pub expected_status: Vec<u16>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: "/health".to_string(),
            interval_secs: 10,
            timeout_ms: 5000,
            expected_status: vec![200],
        }
    }
}

//═══════════════════════════════════════════════════════════════════════════
// SERVICE REGISTRY IMPLEMENTATION
//═══════════════════════════════════════════════════════════════════════════

impl ServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new service in the mesh
    ///
    /// WHY: When we deploy a service via the runner, or when a partner
    /// deploys on our infrastructure, we register it here so tenants can
    /// use it in their orchestration queries.
    pub async fn register_service(&self, service: ManagedService) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        if services.contains_key(&service.id) {
            return Err(RegistryError::AlreadyExists(service.id.clone()));
        }

        tracing::info!(
            service_id = %service.id,
            service_name = %service.name,
            endpoints = service.endpoints.len(),
            "Registering service in mesh"
        );

        services.insert(service.id.clone(), service);
        Ok(())
    }

    /// Unregister a service from the mesh
    pub async fn unregister_service(&self, service_id: &str) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        services
            .remove(service_id)
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        tracing::info!(service_id = %service_id, "Unregistered service from mesh");
        Ok(())
    }

    /// Get a service by ID
    pub async fn get_service(&self, service_id: &str) -> Option<ManagedService> {
        let services = self.services.read().await;
        services.get(service_id).cloned()
    }

    /// List all registered services
    pub async fn list_services(&self) -> Vec<ManagedService> {
        let services = self.services.read().await;
        services.values().cloned().collect()
    }

    /// List services by category
    pub async fn list_by_category(&self, category: &ServiceCategory) -> Vec<ManagedService> {
        let services = self.services.read().await;
        services
            .values()
            .filter(|s| &s.category == category)
            .cloned()
            .collect()
    }

    /// Update health status for an endpoint
    ///
    /// WHY: The health checker continuously monitors endpoints and updates
    /// their status. This is used by the router to avoid unhealthy endpoints.
    pub async fn update_endpoint_health(
        &self,
        service_id: &str,
        endpoint_id: &str,
        health: HealthStatus,
    ) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        let service = services
            .get_mut(service_id)
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        let endpoint = service
            .endpoints
            .iter_mut()
            .find(|e| e.id == endpoint_id)
            .ok_or_else(|| RegistryError::EndpointNotFound(endpoint_id.to_string()))?;

        endpoint.health = health;
        Ok(())
    }

    /// Update latency stats for an endpoint
    ///
    /// WHY: The latency tracker continuously measures response times.
    /// This is used by the router for lowest-latency routing.
    pub async fn update_endpoint_latency(
        &self,
        service_id: &str,
        endpoint_id: &str,
        latency: LatencyStats,
    ) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        let service = services
            .get_mut(service_id)
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        let endpoint = service
            .endpoints
            .iter_mut()
            .find(|e| e.id == endpoint_id)
            .ok_or_else(|| RegistryError::EndpointNotFound(endpoint_id.to_string()))?;

        endpoint.latency = latency;
        Ok(())
    }

    /// Get healthy endpoints for a service
    ///
    /// WHY: The router needs to know which endpoints can receive traffic.
    /// This filters out unhealthy endpoints.
    pub async fn get_healthy_endpoints(&self, service_id: &str) -> Vec<ManagedEndpoint> {
        let services = self.services.read().await;

        services
            .get(service_id)
            .map(|service| {
                service
                    .endpoints
                    .iter()
                    .filter(|e| e.health.is_available())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

//═══════════════════════════════════════════════════════════════════════════
// ERROR TYPES
//═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Service '{0}' not found in registry")]
    NotFound(String),

    #[error("Service '{0}' already exists in registry")]
    AlreadyExists(String),

    #[error("Endpoint '{0}' not found")]
    EndpointNotFound(String),

    #[error("Invalid service configuration: {0}")]
    InvalidConfig(String),
}
