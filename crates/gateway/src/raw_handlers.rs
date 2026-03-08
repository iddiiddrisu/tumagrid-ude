/*!
 * Raw Tokio/Tower Handlers
 *
 * WHY THIS EXISTS:
 * ================
 * Axum's Handler trait can be restrictive for complex async logic.
 * This module provides raw Tower services that bypass Axum's type system
 * while still integrating with the Axum router.
 *
 * These handlers use pure Tokio/async-rust without Axum's magic.
 */

use crate::handlers::orchestration;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use tower::Service;

/// Raw orchestration execute handler
///
/// WHY: Axum's Handler trait wouldn't accept the orchestration logic,
/// so we implement it as a raw Tower service instead.
#[derive(Clone)]
pub struct OrchestrationExecuteService {
    pub state: Arc<AppState>,
}

impl OrchestrationExecuteService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn handle_request(&self, req: Request) -> Response<Body> {
        // Extract path parameters manually
        let path = req.uri().path();
        let segments: Vec<&str> = path.split('/').collect();

        // Expected: /v1/api/{project}/orchestration/{query_id}/execute
        if segments.len() < 6 {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid path - expected /v1/api/{project}/orchestration/{query_id}/execute",
            )
                .into_response();
        }

        let project_id = segments[3];
        let query_id = segments[5];

        // Call the pure business logic
        match orchestration::execute_query_impl(&self.state, project_id, query_id).await {
            Ok(response) => {
                let json = serde_json::to_string(&response).unwrap_or_else(|e| {
                    format!(r#"{{"error": "Failed to serialize response: {}"}}"#, e)
                });
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(json))
                    .unwrap()
            }
            Err(err) => err.into_response(),
        }
    }
}

impl Service<Request> for OrchestrationExecuteService {
    type Response = Response<Body>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let service = self.clone();
        Box::pin(async move { Ok(service.handle_request(req).await) })
    }
}
