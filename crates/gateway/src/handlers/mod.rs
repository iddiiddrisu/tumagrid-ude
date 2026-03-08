pub mod crud;
pub mod health;
pub mod mesh;
pub mod metrics;
pub mod orchestration;

use axum::http::HeaderMap;
use ude_core::{error::AuthError, *};
use std::time::Duration;

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
#[allow(dead_code)]
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
