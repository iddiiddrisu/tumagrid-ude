/*!
 * Service Mesh Executor
 *
 * WHY THIS EXISTS:
 * ================
 * The ServiceMeshExecutor is a DataSourceExecutor that routes requests to
 * services in the mesh using smart routing (health + latency aware).
 *
 * CRITICAL DISTINCTION:
 * - Use ServiceMeshExecutor for services WE deployed and control
 * - Use RestApiExecutor for external APIs and legacy systems
 *
 * This executor:
 * - Uses the router to pick the best endpoint
 * - Executes the HTTP request
 * - Handles retries and timeouts
 * - Records metrics for monitoring
 */

use super::router::ServiceMeshRouter;
use async_trait::async_trait;
use ude_core::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;

//═══════════════════════════════════════════════════════════════════════════
// SERVICE MESH EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for managed services in the mesh
///
/// WHY: This executor uses smart routing to pick the best endpoint for each
/// request, based on health and latency. This is only possible because we
/// deployed these services and have full visibility into their state.
pub struct ServiceMeshExecutor {
    router: Arc<ServiceMeshRouter>,
    client: reqwest::Client,
}

impl ServiceMeshExecutor {
    /// Create a new service mesh executor
    pub fn new(router: Arc<ServiceMeshRouter>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client for service mesh");

        Self { router, client }
    }
}

#[async_trait]
impl crate::orchestration::DataSourceExecutor for ServiceMeshExecutor {
    async fn execute(
        &self,
        _ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<crate::orchestration::DataSourceResult> {
        let start = Instant::now();

        // Default routing strategy (needs to live long enough)
        let default_routing = RoutingStrategy::default();

        // Extract service mesh configuration
        let (service_id, path, method, headers, body, routing) = match &query.source {
            DataSourceConfig::ServiceMesh {
                service,
                path,
                method,
                headers,
                body,
                routing_preference,
            } => (
                service,
                path,
                method,
                headers,
                body,
                routing_preference.as_ref().unwrap_or(&default_routing),
            ),
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for ServiceMeshExecutor".to_string(),
                ))
            }
        };

        tracing::debug!(
            source_id = %query.id,
            service = %service_id,
            path = %path,
            method = %method,
            routing_strategy = ?routing,
            "Executing service mesh request"
        );

        // Route to best endpoint
        let endpoint = self
            .router
            .route(service_id, routing)
            .await
            .map_err(|e| Error::Internal(format!("Routing failed: {}", e)))?;

        tracing::debug!(
            source_id = %query.id,
            service = %service_id,
            endpoint_id = %endpoint.id,
            endpoint_url = %endpoint.url,
            region = %endpoint.region,
            latency_p50 = endpoint.latency.p50_ms,
            "Routed to endpoint"
        );

        // Build full URL
        let url = format!("{}{}", endpoint.url.trim_end_matches('/'), path);

        // Resolve template variables in headers and body
        let resolved_headers = Self::resolve_templates_in_map(headers, resolved_params)?;
        let resolved_body = if let Some(b) = body {
            Some(Self::resolve_templates_value(b, resolved_params)?)
        } else {
            None
        };

        // Build request
        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            "PATCH" => self.client.patch(&url),
            _ => {
                return Err(Error::Validation {
                    field: "method".to_string(),
                    message: format!("Unsupported HTTP method: {}", method),
                })
            }
        };

        // Add headers
        for (key, value) in &resolved_headers {
            request = request.header(key, value);
        }

        // Add body
        if let Some(body_data) = resolved_body {
            request = request.json(&body_data);
        }

        // Execute with retry
        let default_retry = RetryConfig::default();
        let retry_config = query.retry.as_ref().unwrap_or(&default_retry);
        let mut retries = 0;
        let mut last_error = None;

        for attempt in 0..=retry_config.max_retries {
            if attempt > 0 {
                let backoff = retry_config.initial_backoff_ms
                    * (retry_config.multiplier.powi(attempt as i32) as u64);
                let backoff = backoff.min(retry_config.max_backoff_ms);

                tracing::warn!(
                    source_id = %query.id,
                    service = %service_id,
                    endpoint = %endpoint.id,
                    attempt = attempt + 1,
                    backoff_ms = backoff,
                    "Retrying service mesh request"
                );

                tokio::time::sleep(Duration::from_millis(backoff)).await;
                retries = attempt;
            }

            // Execute request
            match request.try_clone().unwrap().send().await {
                Ok(response) if response.status().is_success() => {
                    let data = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|e| Error::Internal(format!("Failed to parse response: {}", e)))?;

                    let duration = start.elapsed();

                    tracing::info!(
                        source_id = %query.id,
                        service = %service_id,
                        endpoint = %endpoint.id,
                        duration_ms = duration.as_millis(),
                        retries = retries,
                        "Service mesh request succeeded"
                    );

                    return Ok(crate::orchestration::DataSourceResult {
                        source_id: query.id.clone(),
                        data,
                        metadata: crate::orchestration::ExecutionMetadata {
                            duration_ms: duration.as_millis() as u64,
                            from_cache: false,
                            retries,
                            was_batched: false,
                            warnings: vec![],
                        },
                    });
                }

                Ok(response) => {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();

                    last_error = Some(Error::Internal(format!(
                        "Service returned error status {}: {}",
                        status, error_body
                    )));

                    // Don't retry 4xx errors (client errors)
                    if status.is_client_error() {
                        break;
                    }
                }

                Err(e) => {
                    last_error = Some(Error::Internal(format!("Request failed: {}", e)));
                }
            }
        }

        tracing::error!(
            source_id = %query.id,
            service = %service_id,
            endpoint = %endpoint.id,
            retries = retries,
            "Service mesh request failed after retries"
        );

        Err(last_error.unwrap_or_else(|| Error::Internal("Request failed".to_string())))
    }
}

impl ServiceMeshExecutor {
    /// Resolve template variables in a map
    fn resolve_templates_in_map(
        map: &HashMap<String, String>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<HashMap<String, String>> {
        let mut resolved = HashMap::new();
        for (k, v) in map {
            let resolved_value = if v.starts_with("${") && v.ends_with('}') {
                // Extract path: "${user.id}" -> "user.id"
                let path = &v[2..v.len() - 1];
                let val = Self::resolve_path(path, params)?;
                if let Some(s) = val.as_str() {
                    s.to_string()
                } else {
                    val.to_string().trim_matches('"').to_string()
                }
            } else {
                v.clone()
            };
            resolved.insert(k.clone(), resolved_value);
        }
        Ok(resolved)
    }

    /// Resolve template variables in a JSON value
    fn resolve_templates_value(
        value: &serde_json::Value,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match value {
            Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
                let path = &s[2..s.len() - 1];
                Self::resolve_path(path, params)
            }
            Value::Object(map) => {
                let mut resolved = serde_json::Map::new();
                for (k, v) in map {
                    resolved.insert(k.clone(), Self::resolve_templates_value(v, params)?);
                }
                Ok(Value::Object(resolved))
            }
            Value::Array(arr) => {
                let resolved: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::resolve_templates_value(v, params))
                    .collect();
                Ok(Value::Array(resolved?))
            }
            _ => Ok(value.clone()),
        }
    }

    /// Resolve a path in the parameters
    fn resolve_path(
        path: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = params.get(parts[0]).ok_or_else(|| Error::Validation {
            field: path.to_string(),
            message: format!("Parameter '{}' not found", parts[0]),
        })?;

        for part in &parts[1..] {
            current = current.get(part).ok_or_else(|| Error::Validation {
                field: path.to_string(),
                message: format!("Path '{}' not found", path),
            })?;
        }

        Ok(current.clone())
    }
}
