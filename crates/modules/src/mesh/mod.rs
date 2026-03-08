/*!
 * Service Mesh Module
 *
 * WHY THIS EXISTS:
 * ================
 * The service mesh provides intelligent routing for services that are deployed
 * and controlled by the SpaceCloud platform. This is a CRITICAL distinction:
 *
 * **ServiceMesh (This Module):**
 * - For services WE deploy via the runner
 * - Multi-region by design
 * - Health + latency monitoring we trust
 * - Smart routing (lowest-latency, region-affinity, etc.)
 * - Part of our orchestration cluster
 *
 * **RestApi (Separate Executor):**
 * - For external APIs (Stripe, Twilio, etc.)
 * - For legacy VMs not in our mesh
 * - For services not deployed via our runner
 * - No smart routing (just retry)
 * - Black-box external services
 *
 * ARCHITECTURE:
 * ```
 * ┌──────────────────────────────────────────────────────────┐
 * │  Tenant Request                                          │
 * └───────────────────────┬──────────────────────────────────┘
 *                         │
 *                         ▼
 * ┌──────────────────────────────────────────────────────────┐
 * │  ServiceMeshExecutor (orchestration data source)         │
 * └───────────────────────┬──────────────────────────────────┘
 *                         │
 *                         ▼
 * ┌──────────────────────────────────────────────────────────┐
 * │  ServiceMeshRouter                                       │
 * │  • Check ServiceRegistry for available endpoints         │
 * │  • Filter by health status (HealthChecker)              │
 * │  • Select by latency (LatencyTracker)                   │
 * │  • Apply routing strategy                                │
 * └───────────────────────┬──────────────────────────────────┘
 *                         │
 *                         ▼
 * ┌──────────────────────────────────────────────────────────┐
 * │  Selected Endpoint                                       │
 * │  (service deployed to our infra)                         │
 * └──────────────────────────────────────────────────────────┘
 * ```
 *
 * USAGE EXAMPLE:
 * ```yaml
 * # Tenant config for managed service
 * sources:
 *   - id: payment
 *     type: servicemesh          # ← Uses mesh executor
 *     service: hubtel-payments   # ← Service ID in registry
 *     path: /v1/charge
 *     method: POST
 *     body:
 *       amount: "${order.amount}"
 *       phone: "${customer.phone}"
 *
 * # Tenant config for external API
 * sources:
 *   - id: stripe
 *     type: restapi              # ← Uses simple REST executor
 *     url: https://api.stripe.com/v1/charges
 *     method: POST
 *     # No smart routing, just retry
 * ```
 */

mod executor;
mod health;
mod latency;
mod registry;
mod router;

pub use executor::ServiceMeshExecutor;
pub use health::{HealthChecker, HealthStats};
pub use latency::{EndpointLatencyInfo, LatencyTracker};
pub use registry::{
    DeploymentInfo, DeploymentMethod, EndpointId, HealthCheckConfig, HealthStatus, LatencyStats,
    ManagedEndpoint, ManagedService, RegistryError, ServiceCategory, ServiceConfig, ServiceId,
    ServiceRegistry,
};
pub use router::{EndpointSummary, RouteError, RoutingStats, ServiceMeshRouter};

// Re-export RoutingStrategy from core
pub use ude_core::RoutingStrategy;

use std::sync::Arc;

//═══════════════════════════════════════════════════════════════════════════
// SERVICE MESH MANAGER
//═══════════════════════════════════════════════════════════════════════════

/// Main entry point for the service mesh
///
/// WHY: This bundles together all the mesh components (registry, health checker,
/// latency tracker, router, executor) into a single easy-to-use interface.
///
/// This is what the gateway creates and uses for orchestration.
pub struct ServiceMesh {
    pub registry: Arc<ServiceRegistry>,
    pub health_checker: Arc<HealthChecker>,
    pub latency_tracker: Arc<LatencyTracker>,
    pub router: Arc<ServiceMeshRouter>,
    pub executor: Arc<ServiceMeshExecutor>,
}

impl ServiceMesh {
    /// Create a new service mesh
    pub fn new() -> Self {
        let registry = Arc::new(ServiceRegistry::new());
        let health_checker = Arc::new(HealthChecker::new(Arc::clone(&registry)));
        let latency_tracker = Arc::new(LatencyTracker::new(Arc::clone(&registry)));
        let router = Arc::new(ServiceMeshRouter::new(Arc::clone(&registry)));
        let executor = Arc::new(ServiceMeshExecutor::new(Arc::clone(&router)));

        Self {
            registry,
            health_checker,
            latency_tracker,
            router,
            executor,
        }
    }

    /// Start background tasks (health checking, latency tracking)
    ///
    /// WHY: The mesh needs continuous monitoring to make smart routing decisions.
    /// This starts the background tasks that keep health and latency data fresh.
    ///
    /// Call this once during gateway startup.
    pub fn start_background_tasks(&self) -> ServiceMeshHandles {
        let health_handle = Arc::clone(&self.health_checker).start();
        let latency_handle = Arc::clone(&self.latency_tracker).start();

        tracing::info!("Service mesh background tasks started");

        ServiceMeshHandles {
            health_checker: health_handle,
            latency_tracker: latency_handle,
        }
    }

    /// Register a new service in the mesh
    ///
    /// WHY: When we deploy a service via the runner, or when a partner deploys
    /// on our infrastructure, we register it here. Then tenants can use it in
    /// their orchestration queries.
    pub async fn register_service(&self, service: ManagedService) -> Result<(), RegistryError> {
        self.registry.register_service(service).await
    }

    /// Get the executor for use in orchestration
    ///
    /// WHY: The orchestration engine needs a DataSourceExecutor to route
    /// requests through the mesh.
    pub fn executor(&self) -> Arc<ServiceMeshExecutor> {
        Arc::clone(&self.executor)
    }
}

impl Default for ServiceMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Handles for background tasks
///
/// WHY: We return these handles so the caller can await them if needed
/// (e.g., for graceful shutdown).
pub struct ServiceMeshHandles {
    pub health_checker: tokio::task::JoinHandle<()>,
    pub latency_tracker: tokio::task::JoinHandle<()>,
}

//═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE FUNCTIONS
//═══════════════════════════════════════════════════════════════════════════

/// Create a service mesh with default configuration
pub fn create_service_mesh() -> ServiceMesh {
    ServiceMesh::new()
}

/// Create a managed service builder
///
/// WHY: Convenience for building ManagedService instances.
pub fn service(id: impl Into<String>, name: impl Into<String>) -> ManagedServiceBuilder {
    ManagedServiceBuilder::new(id.into(), name.into())
}

/// Builder for ManagedService
pub struct ManagedServiceBuilder {
    id: String,
    name: String,
    category: ServiceCategory,
    endpoints: Vec<ManagedEndpoint>,
    deployment: Option<DeploymentInfo>,
    config: ServiceConfig,
}

impl ManagedServiceBuilder {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            category: ServiceCategory::Custom("default".to_string()),
            endpoints: Vec::new(),
            deployment: None,
            config: ServiceConfig::default(),
        }
    }

    pub fn category(mut self, category: ServiceCategory) -> Self {
        self.category = category;
        self
    }

    pub fn endpoint(mut self, endpoint: ManagedEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn endpoints(mut self, endpoints: Vec<ManagedEndpoint>) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn deployment(mut self, deployment: DeploymentInfo) -> Self {
        self.deployment = Some(deployment);
        self
    }

    pub fn config(mut self, config: ServiceConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> ManagedService {
        ManagedService {
            id: self.id,
            name: self.name,
            category: self.category,
            endpoints: self.endpoints,
            deployment: self.deployment.unwrap_or(DeploymentInfo {
                method: DeploymentMethod::Manual,
                runner_version: None,
                image: None,
                version: None,
            }),
            config: self.config,
            registered_at: chrono::Utc::now(),
        }
    }
}

/// Builder for ManagedEndpoint
pub fn endpoint(id: impl Into<String>, url: impl Into<String>, region: impl Into<String>) -> ManagedEndpoint {
    ManagedEndpoint {
        id: id.into(),
        service_id: String::new(), // Will be set by service
        url: url.into(),
        region: region.into(),
        health: HealthStatus::Unknown,
        latency: LatencyStats::default(),
        node_id: None,
        deployed_at: chrono::Utc::now(),
        weight: 1.0,
    }
}
