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
    http::StatusCode,
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

/// Execute a composite query
///
/// WHY: This is the main API for orchestration. Clients send a single request
/// and get back data composed from multiple sources.
///
/// ROUTE: POST /v1/api/:project/orchestration/:queryId
pub async fn execute_query(
    State(state): State<Arc<AppState>>,
    Path((project_id, query_id)): Path<(String, String)>,
    Json(_request): Json<ExecuteQueryRequest>,
) -> std::result::Result<impl IntoResponse, AppError> {
    let _start = std::time::Instant::now();

    tracing::info!(
        project_id = %project_id,
        query_id = %query_id,
        "Executing composite query"
    );

    // Get project module
    let project = state
        .projects
        .read()
        .get(&project_id)
        .cloned()
        .ok_or_else(|| Error::NotFound {
            resource_type: "project".to_string(),
            id: project_id.clone(),
        })?;

    // Get orchestration module
    let _orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // Get the composite query from configuration
    // TODO: Load composite queries from config
    // For now, return error indicating queries need to be configured
    return Err::<Json<ExecuteQueryResponse>, _>(Error::Validation {
        field: "queryId".to_string(),
        message: format!(
            "Composite query '{}' not found. Configure queries in config.yaml under compositeQueries",
            query_id
        ),
    }
    .into());

    // Code for when composite queries are loaded from config:
    /*
    let query = orchestration.get_query(&query_id)
        .ok_or_else(|| Error::NotFound {
            resource: "composite_query".to_string(),
            id: query_id.clone(),
        })?;

    // Create context
    let ctx = Context {
        project_id: project_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        ..Default::default()
    };

    // Execute the query
    let result = orchestration.execute(&ctx, query, request.args).await?;

    let total_duration = start.elapsed();

    // Build metadata
    let metadata = QueryExecutionMetadata {
        total_duration_ms: total_duration.as_millis() as u64,
        num_sources: query.sources.len(),
        num_stages: 0, // TODO: Extract from execution plan
        used_cache: false, // TODO: Track cache usage
        warnings: vec![],
    };

    Ok((
        StatusCode::OK,
        Json(ExecuteQueryResponse {
            data: result,
            metadata,
        }),
    ))
    */
}

/// List all available composite queries for a project
///
/// WHY: Helps with discovery - clients can see what queries are available
///
/// ROUTE: GET /v1/api/:project/orchestration
pub async fn list_queries(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> std::result::Result<impl IntoResponse, AppError> {
    tracing::debug!(
        project_id = %project_id,
        "Listing composite queries"
    );

    // Get project module
    let project = state
        .projects
        .read()
        .get(&project_id)
        .cloned()
        .ok_or_else(|| Error::NotFound {
            resource_type: "project".to_string(),
            id: project_id.clone(),
        })?;

    // Get orchestration module
    let _orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // TODO: Return list of configured queries
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "queries": []
        })),
    ))
}

/// Get details about a specific composite query
///
/// WHY: Helps with documentation - show what sources and composition a query uses
///
/// ROUTE: GET /v1/api/:project/orchestration/:queryId
pub async fn get_query_info(
    State(state): State<Arc<AppState>>,
    Path((project_id, query_id)): Path<(String, String)>,
) -> std::result::Result<impl IntoResponse, AppError> {
    tracing::debug!(
        project_id = %project_id,
        query_id = %query_id,
        "Getting composite query info"
    );

    // Get project module
    let project = state
        .projects
        .read()
        .get(&project_id)
        .cloned()
        .ok_or_else(|| Error::NotFound {
            resource_type: "project".to_string(),
            id: project_id.clone(),
        })?;

    // Get orchestration module
    let _orchestration = project
        .orchestration
        .as_ref()
        .ok_or_else(|| Error::Internal("Orchestration module not initialized".to_string()))?;

    // TODO: Return query details from configuration
    Err::<Json<serde_json::Value>, _>(
        Error::NotFound {
            resource_type: "composite_query".to_string(),
            id: query_id,
        }
        .into(),
    )
}

/// Error wrapper for HTTP responses
#[derive(Debug)]
pub struct AppError(Error);

impl From<Error> for AppError {
    fn from(err: Error) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self.0 {
            Error::NotFound { resource_type, id } => (
                StatusCode::NOT_FOUND,
                format!("{} '{}' not found", resource_type, id),
            ),
            Error::Validation { field, message } => (
                StatusCode::BAD_REQUEST,
                format!("Validation error on '{}': {}", field, message),
            ),
            Error::Auth(ref err) => (StatusCode::UNAUTHORIZED, format!("Auth error: {}", err)),
            Error::Timeout(_) => (StatusCode::REQUEST_TIMEOUT, "Request timeout".to_string()),
            Error::Network(ref err) => (StatusCode::BAD_GATEWAY, format!("Network error: {}", err)),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
