pub mod crud;
pub mod health;
pub mod mesh;
pub mod metrics;
pub mod orchestration;

use axum::http::HeaderMap;
use ude_core::{error::AuthError, *};
use std::time::Duration;
use ude_managers::ProjectModules;

/// Extract context from HTTP headers
pub fn extract_context(headers: &HeaderMap) -> Context {
    let request_id = headers
        .get("X-Request-ID")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Context {
        request_id,
        timeout: Some(Duration::from_secs(30)),
        claims: None,
        metadata: std::collections::HashMap::new(),
    }
}

/// Extract bearer token from Authorization header
pub fn extract_token(headers: &HeaderMap) -> Result<String> {
    headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::Auth(AuthError::InvalidToken(
                "Missing Authorization header".to_string(),
            ))
        })
}

/// Validate namespace access for a project
pub fn validate_namespace_access(
    project_id: &str,
    modules: &ProjectModules,
    claims: &TokenClaims,
) -> Result<()> {
    let project_namespace = &modules.project_config.namespace;

    // Check if user has access to this namespace
    if !claims.has_namespace_access(project_namespace) {
        return Err(Error::Auth(AuthError::InvalidToken(format!(
            "Access denied: project '{}' belongs to namespace '{}', but user only has access to namespaces: [{}]",
            project_id,
            project_namespace,
            claims.get_namespaces().join(", ")
        ))));
    }

    tracing::debug!(
        project_id = %project_id,
        namespace = %project_namespace,
        user_namespaces = ?claims.get_namespaces(),
        "Namespace access validated"
    );

    Ok(())
}
