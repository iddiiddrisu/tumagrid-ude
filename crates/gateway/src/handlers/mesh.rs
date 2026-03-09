/*!
 * Service Mesh Management Handlers
 *
 * WHY THIS EXISTS:
 * ================
 * These handlers provide HTTP APIs for managing the service mesh:
 * - Registering new services
 * - Listing services
 * - Checking service health
 * - Viewing routing statistics
 *
 * This is how partners and operators interact with the mesh to:
 * - Register services they've deployed
 * - Monitor service health
 * - Debug routing decisions
 */

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use ude_modules::{
    DeploymentInfo, DeploymentMethod, ManagedEndpoint, ManagedService, ServiceCategory,
    ServiceConfig,
};
use std::sync::Arc;

//═══════════════════════════════════════════════════════════════════════════
// LIST SERVICES
//═══════════════════════════════════════════════════════════════════════════

/// List all registered services in the mesh
///
/// GET /mesh/services
pub async fn list_services(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let services = state.mesh().registry.list_services().await;

    Json(ListServicesResponse {
        count: services.len(),
        services: services
            .into_iter()
            .map(|s| ServiceSummary {
                id: s.id,
                name: s.name,
                category: s.category,
                endpoint_count: s.endpoints.len(),
                registered_at: s.registered_at,
            })
            .collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct ListServicesResponse {
    count: usize,
    services: Vec<ServiceSummary>,
}

#[derive(Debug, Serialize)]
pub struct ServiceSummary {
    id: String,
    name: String,
    category: ServiceCategory,
    endpoint_count: usize,
    registered_at: chrono::DateTime<chrono::Utc>,
}

//═══════════════════════════════════════════════════════════════════════════
// GET SERVICE DETAILS
//═══════════════════════════════════════════════════════════════════════════

/// Get detailed information about a specific service
///
/// GET /mesh/services/:service_id
pub async fn get_service(
    State(state): State<Arc<AppState>>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceDetails>, (StatusCode, String)> {
    let service = state
        .mesh()
        .registry
        .get_service(&service_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Service '{}' not found", service_id),
            )
        })?;

    // Get health stats
    let health_stats = state
        .mesh()
        .health_checker
        .get_service_health_stats(&service_id)
        .await;

    // Get routing stats
    let routing_stats = state.mesh().router.get_routing_stats(&service_id).await;

    Ok(Json(ServiceDetails {
        service,
        health_stats,
        routing_stats,
    }))
}

#[derive(Debug, Serialize)]
pub struct ServiceDetails {
    service: ManagedService,
    health_stats: Option<ude_modules::HealthStats>,
    routing_stats: Option<ude_modules::RoutingStats>,
}

//═══════════════════════════════════════════════════════════════════════════
// REGISTER SERVICE
//═══════════════════════════════════════════════════════════════════════════

/// Register a new service in the mesh
///
/// POST /mesh/services
pub async fn register_service(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterServiceRequest>,
) -> Result<Json<RegisterServiceResponse>, (StatusCode, String)> {
    // Build service from request
    let service = ManagedService {
        id: request.id.clone(),
        name: request.name,
        category: request.category,
        endpoints: request.endpoints,
        deployment: request.deployment.unwrap_or_else(|| DeploymentInfo {
            method: DeploymentMethod::Manual,
            runner_version: None,
            image: None,
            version: None,
        }),
        config: request.config.unwrap_or_default(),
        registered_at: chrono::Utc::now(),
    };

    // Register in mesh
    state
        .mesh()
        .register_service(service)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    tracing::info!(service_id = %request.id, "Service registered in mesh");

    Ok(Json(RegisterServiceResponse {
        service_id: request.id,
        message: "Service registered successfully".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RegisterServiceRequest {
    pub id: String,
    pub name: String,
    pub category: ServiceCategory,
    pub endpoints: Vec<ManagedEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<DeploymentInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ServiceConfig>,
}

#[derive(Debug, Serialize)]
pub struct RegisterServiceResponse {
    service_id: String,
    message: String,
}

//═══════════════════════════════════════════════════════════════════════════
// UNREGISTER SERVICE
//═══════════════════════════════════════════════════════════════════════════

/// Unregister a service from the mesh
///
/// DELETE /mesh/services/:service_id
pub async fn unregister_service(
    State(state): State<Arc<AppState>>,
    Path(service_id): Path<String>,
) -> Result<Json<UnregisterServiceResponse>, (StatusCode, String)> {
    state
        .mesh()
        .registry
        .unregister_service(&service_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    tracing::info!(service_id = %service_id, "Service unregistered from mesh");

    Ok(Json(UnregisterServiceResponse {
        service_id,
        message: "Service unregistered successfully".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct UnregisterServiceResponse {
    service_id: String,
    message: String,
}

//═══════════════════════════════════════════════════════════════════════════
// GET SERVICE HEALTH
//═══════════════════════════════════════════════════════════════════════════

/// Get health status for a service
///
/// GET /mesh/services/:service_id/health
pub async fn get_service_health(
    State(state): State<Arc<AppState>>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceHealthResponse>, (StatusCode, String)> {
    let stats = state
        .mesh()
        .health_checker
        .get_service_health_stats(&service_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Service '{}' not found", service_id),
            )
        })?;

    let endpoints = state
        .mesh()
        .registry
        .get_service(&service_id)
        .await
        .map(|s| s.endpoints)
        .unwrap_or_default();

    Ok(Json(ServiceHealthResponse {
        service_id,
        stats,
        endpoints: endpoints
            .into_iter()
            .map(|e| EndpointHealth {
                id: e.id,
                url: e.url,
                region: e.region,
                health: e.health,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize)]
pub struct ServiceHealthResponse {
    service_id: String,
    stats: ude_modules::HealthStats,
    endpoints: Vec<EndpointHealth>,
}

#[derive(Debug, Serialize)]
pub struct EndpointHealth {
    id: String,
    url: String,
    region: String,
    health: ude_modules::HealthStatus,
}

//═══════════════════════════════════════════════════════════════════════════
// GET SERVICE LATENCY
//═══════════════════════════════════════════════════════════════════════════

/// Get latency statistics for a service
///
/// GET /mesh/services/:service_id/latency
pub async fn get_service_latency(
    State(state): State<Arc<AppState>>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceLatencyResponse>, (StatusCode, String)> {
    let latency_info = state
        .mesh()
        .latency_tracker
        .get_service_latency_stats(&service_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Service '{}' not found", service_id),
            )
        })?;

    Ok(Json(ServiceLatencyResponse {
        service_id,
        endpoints: latency_info,
    }))
}

#[derive(Debug, Serialize)]
pub struct ServiceLatencyResponse {
    service_id: String,
    endpoints: Vec<ude_modules::EndpointLatencyInfo>,
}
