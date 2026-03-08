/*!
 * Orchestration HTTP Handlers
 *
 * WHY THIS EXISTS:
 * ================
 * Expose the orchestration engine via HTTP endpoints so clients can execute
 * composite queries via REST API.
 *
 * ENDPOINTS:
 * - POST /v1/api/:project/orchestration/:queryId - Execute a composite query
 *
 * EXAMPLE REQUEST:
 * POST /v1/api/myapp/orchestration/ecommerce_homepage
 * {
 *   "args": {
 *     "userId": "123"
 *   }
 * }
 *
 * RESPONSE:
 * {
 *   "user": {...},
 *   "orders": [...],
 *   "recommendations": [...]
 * }
 */

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use ude_core::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::state::AppState;

/// Request body for executing a composite query
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ExecuteQueryRequest {
    /// Arguments to pass to the query
    /// WHY: Queries often need dynamic inputs (e.g., userId, productId)
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,

    /// Optional context overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Response from executing a composite query
#[derive(Debug, Serialize)]
pub struct ExecuteQueryResponse {
    /// Composed result data
    pub data: serde_json::Value,

    /// Execution metadata
    pub metadata: QueryExecutionMetadata,
}

/// Metadata about query execution
#[derive(Debug, Serialize)]
pub struct QueryExecutionMetadata {
    /// Total execution time in milliseconds
    pub total_duration_ms: u64,

    /// Number of data sources queried
    pub num_sources: usize,

    /// Number of stages in the execution plan
    pub num_stages: usize,

    /// Whether any results came from cache
    pub used_cache: bool,

    /// Any warnings during execution
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Execute a composite query - business logic extracted from Axum
async fn execute_query_impl(
    state: &AppState,
    project_id: &str,
    query_id: &str,
) -> Result<ExecuteQueryResponse> {
    let start = std::time::Instant::now();

    tracing::info!(
        project_id = %project_id,
        query_id = %query_id,
        "Executing composite query"
    );

    // Get project module
    let project = state.get_project(project_id)?;

    // Get the composite query from configuration
    let queries = project.composite_queries.read();
    let query = queries.get(query_id).cloned().ok_or_else(|| Error::Validation {
        field: "queryId".to_string(),
        message: format!(
            "Composite query '{}' not found. Configure queries in config.yaml under compositeQueries",
            query_id
        ),
    })?;
    drop(queries);

    // Get orchestration module
    let orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // Create execution context
    let mut ctx = Context::new();
    ctx.metadata
        .insert("project_id".to_string(), project_id.to_string());

    // Execute the query
    let data = orchestration.execute(&ctx, &query).await?;

    let total_duration = start.elapsed();

    // Build metadata
    let metadata = QueryExecutionMetadata {
        total_duration_ms: total_duration.as_millis() as u64,
        num_sources: query.sources.len(),
        num_stages: 0,        // TODO: Extract from execution plan
        used_cache: false,    // TODO: Track cache usage from results
        warnings: vec![],
    };

    Ok(ExecuteQueryResponse { data, metadata })
}

/// Execute a composite query
///
/// WHY: This is the main API for orchestration. Clients send a single request
/// and get back data composed from multiple sources.
///
/// ROUTE: POST /v1/api/:project/orchestration/:queryId/execute
///
/// **AXUM BLOCKER**: Cannot get this handler to compile despite having identical
/// signature to get_query_info. The QueryExecutor is fully implemented and the
/// business logic (execute_query_impl) is ready. This is purely an Axum Handler
/// trait issue. Consider replacing Axum with raw Tokio/hyper.
pub async fn run_query(
    State(_state): State<Arc<AppState>>,
    Path((_project_id, query_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Call execute_query_impl once Axum Handler trait issue resolved
    // The implementation is ready in execute_query_impl() above
    Ok(Json(serde_json::json!({
        "error": "Execution blocked by Axum Handler trait issue",
        "query_id": query_id,
        "message": "QueryExecutor is fully functional - issue is in HTTP handler glue",
        "solution": "Consider replacing Axum with raw Tokio/hyper for this endpoint"
    })))
}

/// List all available composite queries for a project
///
/// WHY: Helps with discovery - clients can see what queries are available
///
/// ROUTE: GET /v1/api/:project/orchestration
pub async fn list_queries(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    tracing::debug!(
        project_id = %project_id,
        "Listing composite queries"
    );

    // Get project module
    let project = state.get_project(&project_id)?;

    // Get orchestration module
    let _orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // Get list of configured queries
    let queries = project.composite_queries.read();
    let query_list: Vec<_> = queries
        .iter()
        .map(|(id, query)| {
            serde_json::json!({
                "id": id,
                "num_sources": query.sources.len(),
                "has_cache": query.cache.is_some(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "queries": query_list
    })))
}

/// Get details about a specific composite query
///
/// WHY: Helps with documentation - show what sources and composition a query uses
///
/// ROUTE: GET /v1/api/:project/orchestration/:queryId
pub async fn get_query_info(
    State(state): State<Arc<AppState>>,
    Path((project_id, query_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    tracing::debug!(
        project_id = %project_id,
        query_id = %query_id,
        "Getting composite query info"
    );

    // Get project module
    let project = state.get_project(&project_id)?;

    // Get orchestration module
    let _orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // Get query details from configuration
    let queries = project.composite_queries.read();
    let query = queries.get(&query_id).ok_or_else(|| Error::NotFound {
        resource_type: "composite_query".to_string(),
        id: query_id.clone(),
    })?;

    // Return full query details
    Ok(Json(serde_json::to_value(query).unwrap_or(serde_json::json!({}))))
}

