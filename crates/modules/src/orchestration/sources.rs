/*!
 * Data Source Executors
 *
 * WHY THIS EXISTS:
 * ================
 * SpaceForge needs to fetch data from many different types of sources:
 * - SQL databases (Postgres, MySQL, SQL Server)
 * - NoSQL databases (MongoDB)
 * - REST APIs (microservices)
 * - GraphQL services
 * - Serverless functions
 * - Cache (Redis)
 *
 * This module provides a unified interface for executing queries against
 * all these different source types, with features like:
 * - Automatic batching (solving N+1 problems)
 * - Retry with exponential backoff
 * - Timeout handling
 * - Caching
 * - Performance metrics
 */

use async_trait::async_trait;
use ude_core::{error::NetworkError, *};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{timeout, Duration};

//═══════════════════════════════════════════════════════════════════════════
// DATA SOURCE EXECUTOR TRAIT
//═══════════════════════════════════════════════════════════════════════════

/// Trait for executing queries against different data source types
///
/// WHY: Different data sources have different protocols, but we want to
/// treat them uniformly in the query planner and executor.
#[async_trait]
pub trait DataSourceExecutor: Send + Sync {
    /// Execute a query against this data source
    async fn execute(
        &self,
        ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult>;

    /// Whether this source supports batching multiple queries
    ///
    /// WHY: Some APIs support batching (e.g., /products?ids=1,2,3) which
    /// dramatically improves performance for N+1 scenarios.
    fn supports_batch(&self) -> bool {
        false
    }

    /// Execute multiple queries in a batch
    ///
    /// WHY: When fetching related data (e.g., products for each order),
    /// batching reduces N requests to 1 request.
    async fn execute_batch(
        &self,
        _ctx: &Context,
        _queries: &[DataSourceQuery],
        _resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<DataSourceResult>> {
        Err(Error::Internal(
            "Batching not supported for this source".to_string(),
        ))
    }
}

//═══════════════════════════════════════════════════════════════════════════
// DATA SOURCE REGISTRY
//═══════════════════════════════════════════════════════════════════════════

/// Registry of all available data source executors
///
/// WHY: The query executor needs to route queries to the appropriate executor
/// based on the source type. This registry manages all executor instances.
pub struct DataSourceRegistry {
    database: Option<Arc<DatabaseExecutor>>,
    rest_api: Arc<RestApiExecutor>,
    service_mesh: Option<Arc<crate::mesh::ServiceMeshExecutor>>,
    graphql: Arc<GraphQLExecutor>,
    function: Option<Arc<FunctionExecutor>>,
    cache: Option<Arc<CacheExecutor>>,
}

impl DataSourceRegistry {
    pub fn new() -> Self {
        Self {
            database: None,
            rest_api: Arc::new(RestApiExecutor::new()),
            service_mesh: None,
            graphql: Arc::new(GraphQLExecutor::new()),
            function: None,
            cache: None,
        }
    }

    pub fn with_database(mut self, executor: Arc<DatabaseExecutor>) -> Self {
        self.database = Some(executor);
        self
    }

    pub fn with_service_mesh(mut self, executor: Arc<crate::mesh::ServiceMeshExecutor>) -> Self {
        self.service_mesh = Some(executor);
        self
    }

    pub fn with_cache(mut self, executor: Arc<CacheExecutor>) -> Self {
        self.cache = Some(executor);
        self
    }

    /// Get the appropriate executor for a data source
    pub fn get_executor(&self, config: &DataSourceConfig) -> Result<Arc<dyn DataSourceExecutor>> {
        match config {
            DataSourceConfig::Database { .. } => self
                .database
                .as_ref()
                .map(|e| e.clone() as Arc<dyn DataSourceExecutor>)
                .ok_or_else(|| Error::Internal("Database executor not configured".to_string())),
            DataSourceConfig::RestApi { .. } => {
                Ok(self.rest_api.clone() as Arc<dyn DataSourceExecutor>)
            }
            DataSourceConfig::ServiceMesh { .. } => self
                .service_mesh
                .as_ref()
                .map(|e| e.clone() as Arc<dyn DataSourceExecutor>)
                .ok_or_else(|| Error::Internal("Service mesh executor not configured".to_string())),
            DataSourceConfig::GraphQL { .. } => {
                Ok(self.graphql.clone() as Arc<dyn DataSourceExecutor>)
            }
            DataSourceConfig::Function { .. } => self
                .function
                .as_ref()
                .map(|e| e.clone() as Arc<dyn DataSourceExecutor>)
                .ok_or_else(|| Error::Internal("Function executor not configured".to_string())),
            DataSourceConfig::Cache { .. } => self
                .cache
                .as_ref()
                .map(|e| e.clone() as Arc<dyn DataSourceExecutor>)
                .ok_or_else(|| Error::Internal("Cache executor not configured".to_string())),
        }
    }
}

//═══════════════════════════════════════════════════════════════════════════
// DATABASE EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for database queries (reuses existing CRUD module)
///
/// WHY: We already have a great database abstraction layer. This executor
/// just adapts it to the DataSourceExecutor interface.
pub struct DatabaseExecutor {
    crud_module: Arc<crate::crud::CrudModule>,
}

impl DatabaseExecutor {
    pub fn new(crud_module: Arc<crate::crud::CrudModule>) -> Self {
        Self { crud_module }
    }
}

#[async_trait]
impl DataSourceExecutor for DatabaseExecutor {
    async fn execute(
        &self,
        ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        let start = Instant::now();

        let (db_alias, collection, find, options) = match &query.source {
            DataSourceConfig::Database {
                db_alias,
                collection,
                find,
                options,
            } => {
                // Resolve template variables in find clause
                let resolved_find = Self::resolve_templates(find, resolved_params)?;
                (db_alias, collection, resolved_find, options.clone())
            }
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for DatabaseExecutor".to_string(),
                ))
            }
        };

        // Execute query with timeout
        let timeout_duration = Duration::from_millis(query.timeout_ms.unwrap_or(30000));
        let result = timeout(
            timeout_duration,
            self.crud_module.read(
                ctx,
                db_alias,
                collection,
                ReadRequest { find, options },
                RequestParams::default(),
            ),
        )
        .await
        .map_err(|_| Error::Timeout(timeout_duration))??;

        let duration = start.elapsed();

        Ok(DataSourceResult {
            source_id: query.id.clone(),
            data: serde_json::json!(result.data),
            metadata: ExecutionMetadata {
                duration_ms: duration.as_millis() as u64,
                from_cache: false,
                retries: 0,
                was_batched: false,
                warnings: vec![],
            },
        })
    }
}

impl DatabaseExecutor {
    /// Resolve template variables in a JSON value
    ///
    /// WHY: Queries often need to reference data from previous queries.
    /// Example: {"user_id": "${user.id}"} where user comes from a previous query.
    fn resolve_templates(
        value: &serde_json::Value,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match value {
            Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
                // Extract path: "${user.id}" -> "user.id"
                let path = &s[2..s.len() - 1];
                Self::resolve_path(path, params)
            }
            Value::Object(map) => {
                let mut resolved = serde_json::Map::new();
                for (k, v) in map {
                    resolved.insert(k.clone(), Self::resolve_templates(v, params)?);
                }
                Ok(Value::Object(resolved))
            }
            Value::Array(arr) => {
                let resolved: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::resolve_templates(v, params))
                    .collect();
                Ok(Value::Array(resolved?))
            }
            _ => Ok(value.clone()),
        }
    }

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

//═══════════════════════════════════════════════════════════════════════════
// REST API EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for REST API calls
///
/// WHY: Most microservices expose REST APIs. This executor handles HTTP
/// requests with features like batching, retries, and timeout.
pub struct RestApiExecutor {
    client: reqwest::Client,
}

impl RestApiExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
}

#[async_trait]
impl DataSourceExecutor for RestApiExecutor {
    async fn execute(
        &self,
        _ctx: &Context,
        query: &DataSourceQuery,
        _resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        let start = Instant::now();

        let (url, method, headers, params, body, _batch, _batch_field) = match &query.source {
            DataSourceConfig::RestApi {
                url,
                method,
                headers,
                params,
                body,
                batch,
                batch_field,
            } => (url, method, headers, params, body, *batch, batch_field),
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for RestApiExecutor".to_string(),
                ))
            }
        };

        tracing::debug!(
            source_id = %query.id,
            url = %url,
            method = %method,
            "Executing REST API query"
        );

        // Build request
        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            _ => {
                return Err(Error::Validation {
                    field: "method".to_string(),
                    message: format!("Unsupported HTTP method: {}", method),
                })
            }
        };

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Add query parameters
        if !params.is_empty() {
            let params_vec: Vec<_> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or_default()))
                .collect();
            request = request.query(&params_vec);
        }

        // Add body
        if let Some(body_data) = body {
            request = request.json(body_data);
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
                    attempt = attempt + 1,
                    backoff_ms = backoff,
                    "Retrying REST API call"
                );

                tokio::time::sleep(Duration::from_millis(backoff)).await;
                retries = attempt;
            }

            match request.try_clone().unwrap().send().await {
                Ok(response) if response.status().is_success() => {
                    let data = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|e| Error::Network(NetworkError::Request(e.to_string())))?;

                    let duration = start.elapsed();

                    return Ok(DataSourceResult {
                        source_id: query.id.clone(),
                        data,
                        metadata: ExecutionMetadata {
                            duration_ms: duration.as_millis() as u64,
                            from_cache: false,
                            retries,
                            was_batched: false,
                            warnings: vec![],
                        },
                    });
                }
                Ok(response) => {
                    last_error = Some(Error::Network(NetworkError::Request(format!(
                        "HTTP {}: {}",
                        response.status(),
                        response.status().canonical_reason().unwrap_or("Unknown")
                    ))));
                }
                Err(e) => {
                    last_error = Some(Error::Network(NetworkError::Request(e.to_string())));
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| Error::Network(NetworkError::Request("Unknown error".to_string()))))
    }

    fn supports_batch(&self) -> bool {
        true // REST APIs can support batching
    }
}

//═══════════════════════════════════════════════════════════════════════════
// GRAPHQL EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for GraphQL queries
///
/// WHY: Many services expose GraphQL APIs. This executor handles GraphQL
/// requests with features like variable substitution, custom headers, and timeout.
pub struct GraphQLExecutor {
    client: reqwest::Client,
}

impl GraphQLExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
}

#[async_trait]
impl DataSourceExecutor for GraphQLExecutor {
    async fn execute(
        &self,
        _ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        let start = Instant::now();

        let (url, gql_query, variables, headers) = match &query.source {
            DataSourceConfig::GraphQL {
                url,
                query,
                variables,
                headers,
            } => {
                // Resolve template variables in query and variables
                let resolved_query = Self::resolve_string_templates(query, resolved_params)?;
                let resolved_variables = Self::resolve_map_templates(variables, resolved_params)?;
                (url, resolved_query, resolved_variables, headers)
            }
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for GraphQLExecutor".to_string(),
                ))
            }
        };

        tracing::debug!(
            source_id = %query.id,
            url = %url,
            "Executing GraphQL query"
        );

        // Build GraphQL request body
        let request_body = serde_json::json!({
            "query": gql_query,
            "variables": variables,
        });

        // Build HTTP request
        let mut request = self.client.post(url);

        // Add headers
        request = request.header("Content-Type", "application/json");
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Set body
        request = request.json(&request_body);

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
                    attempt = attempt + 1,
                    backoff_ms = backoff,
                    "Retrying GraphQL query"
                );

                tokio::time::sleep(Duration::from_millis(backoff)).await;
                retries = attempt;
            }

            match request.try_clone().unwrap().send().await {
                Ok(response) if response.status().is_success() => {
                    let response_json = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|e| Error::Network(NetworkError::Request(e.to_string())))?;

                    // Check for GraphQL errors
                    if let Some(errors) = response_json.get("errors") {
                        if errors.is_array() && !errors.as_array().unwrap().is_empty() {
                            return Err(Error::Network(NetworkError::Request(format!(
                                "GraphQL errors: {}",
                                errors
                            ))));
                        }
                    }

                    // Extract data field from GraphQL response
                    let data = response_json
                        .get("data")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    let duration = start.elapsed();

                    return Ok(DataSourceResult {
                        source_id: query.id.clone(),
                        data,
                        metadata: ExecutionMetadata {
                            duration_ms: duration.as_millis() as u64,
                            from_cache: false,
                            retries,
                            was_batched: false,
                            warnings: vec![],
                        },
                    });
                }
                Ok(response) => {
                    last_error = Some(Error::Network(NetworkError::Request(format!(
                        "HTTP {}: {}",
                        response.status(),
                        response.status().canonical_reason().unwrap_or("Unknown")
                    ))));
                }
                Err(e) => {
                    last_error = Some(Error::Network(NetworkError::Request(e.to_string())));
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| Error::Network(NetworkError::Request("Unknown error".to_string()))))
    }
}

impl GraphQLExecutor {
    /// Resolve template variables in a string
    fn resolve_string_templates(
        template: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<String> {
        let mut result = template.to_string();

        // Simple template replacement for ${variable} patterns
        // This is a basic implementation - could be enhanced with proper template engine
        for (key, value) in params {
            let placeholder = format!("${{{}}}", key);
            if result.contains(&placeholder) {
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => serde_json::to_string(value).unwrap_or_default(),
                };
                result = result.replace(&placeholder, &replacement);
            }
        }

        Ok(result)
    }

    /// Resolve template variables in a map
    fn resolve_map_templates(
        variables: &HashMap<String, serde_json::Value>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut resolved = HashMap::new();

        for (key, value) in variables {
            resolved.insert(key.clone(), Self::resolve_value_template(value, params)?);
        }

        Ok(resolved)
    }

    /// Resolve template variables in a JSON value
    fn resolve_value_template(
        value: &serde_json::Value,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match value {
            Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
                // Extract path: "${user.id}" -> "user.id"
                let path = &s[2..s.len() - 1];
                Self::resolve_path(path, params)
            }
            Value::Object(map) => {
                let mut resolved = serde_json::Map::new();
                for (k, v) in map {
                    resolved.insert(k.clone(), Self::resolve_value_template(v, params)?);
                }
                Ok(Value::Object(resolved))
            }
            Value::Array(arr) => {
                let resolved: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::resolve_value_template(v, params))
                    .collect();
                Ok(Value::Array(resolved?))
            }
            _ => Ok(value.clone()),
        }
    }

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

//═══════════════════════════════════════════════════════════════════════════
// FUNCTION EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for serverless functions
///
/// WHY: Modern applications often have business logic in serverless functions
/// (AWS Lambda, Cloud Functions, etc.). This executor integrates function results
/// into composite queries.
///
/// DESIGN: Functions are called via HTTP endpoints (function gateway pattern).
/// This works with AWS Lambda Function URLs, Cloud Functions, Azure Functions,
/// or any custom function runtime.
pub struct FunctionExecutor {
    client: reqwest::Client,
    /// Function registry mapping function names to URLs
    function_urls: Arc<parking_lot::RwLock<HashMap<String, String>>>,
}

impl FunctionExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            function_urls: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Register a function URL
    ///
    /// WHY: Function names in config should be logical (e.g., "ml-recommendations")
    /// but actual URLs might be complex (AWS Lambda URLs). This allows mapping.
    pub fn register_function(&self, name: String, url: String) {
        self.function_urls.write().insert(name, url);
    }

    /// Register multiple functions from configuration
    pub fn register_functions(&self, functions: HashMap<String, String>) {
        let mut registry = self.function_urls.write();
        registry.extend(functions);
    }
}

#[async_trait]
impl DataSourceExecutor for FunctionExecutor {
    async fn execute(
        &self,
        _ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        let start = Instant::now();

        let (name, args) = match &query.source {
            DataSourceConfig::Function { name, args } => {
                // Resolve template variables in arguments
                let resolved_args = Self::resolve_args(args, resolved_params)?;
                (name, resolved_args)
            }
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for FunctionExecutor".to_string(),
                ))
            }
        };

        // Look up function URL
        let url = self.function_urls.read()
            .get(name)
            .cloned()
            .ok_or_else(|| Error::Validation {
                field: "function".to_string(),
                message: format!("Function '{}' not registered. Register it using FunctionExecutor::register_function()", name),
            })?;

        tracing::debug!(
            source_id = %query.id,
            function_name = %name,
            url = %url,
            "Executing function"
        );

        // Build request
        let mut request = self.client.post(&url);
        request = request
            .header("Content-Type", "application/json")
            .json(&args);

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
                    function_name = %name,
                    attempt = attempt + 1,
                    backoff_ms = backoff,
                    "Retrying function call"
                );

                tokio::time::sleep(Duration::from_millis(backoff)).await;
                retries = attempt;
            }

            match request.try_clone().unwrap().send().await {
                Ok(response) if response.status().is_success() => {
                    let data = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|e| Error::Network(NetworkError::Request(e.to_string())))?;

                    let duration = start.elapsed();

                    // Log if function was slow
                    let mut warnings = vec![];
                    if duration.as_millis() > 3000 {
                        warnings.push(format!(
                            "Function '{}' took {}ms (slow function warning)",
                            name,
                            duration.as_millis()
                        ));
                    }

                    return Ok(DataSourceResult {
                        source_id: query.id.clone(),
                        data,
                        metadata: ExecutionMetadata {
                            duration_ms: duration.as_millis() as u64,
                            from_cache: false,
                            retries,
                            was_batched: false,
                            warnings,
                        },
                    });
                }
                Ok(response) => {
                    last_error = Some(Error::Network(NetworkError::Request(format!(
                        "HTTP {}: {}",
                        response.status(),
                        response.status().canonical_reason().unwrap_or("Unknown")
                    ))));
                }
                Err(e) => {
                    last_error = Some(Error::Network(NetworkError::Request(e.to_string())));
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| Error::Network(NetworkError::Request("Unknown error".to_string()))))
    }
}

impl FunctionExecutor {
    /// Resolve template variables in function arguments
    fn resolve_args(
        args: &HashMap<String, serde_json::Value>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        let mut resolved = serde_json::Map::new();

        for (key, value) in args {
            resolved.insert(key.clone(), Self::resolve_value(value, params)?);
        }

        Ok(Value::Object(resolved))
    }

    fn resolve_value(
        value: &serde_json::Value,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match value {
            Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
                // Extract path: "${user.id}" -> "user.id"
                let path = &s[2..s.len() - 1];
                Self::resolve_path(path, params)
            }
            Value::Object(map) => {
                let mut resolved = serde_json::Map::new();
                for (k, v) in map {
                    resolved.insert(k.clone(), Self::resolve_value(v, params)?);
                }
                Ok(Value::Object(resolved))
            }
            Value::Array(arr) => {
                let resolved: Result<Vec<_>> =
                    arr.iter().map(|v| Self::resolve_value(v, params)).collect();
                Ok(Value::Array(resolved?))
            }
            _ => Ok(value.clone()),
        }
    }

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

//═══════════════════════════════════════════════════════════════════════════
// CACHE EXECUTOR
//═══════════════════════════════════════════════════════════════════════════

/// Executor for Redis cache queries
///
/// WHY: Cache is critical for performance. Frequently accessed data should come
/// from Redis (sub-millisecond access) instead of databases or APIs.
///
/// FEATURES:
/// - GET from cache
/// - Automatic JSON deserialization
/// - Cache miss handling (returns null instead of error)
/// - Performance tracking
pub struct CacheExecutor {
    /// Redis connection manager for pooled connections
    /// WHY: Connection pooling prevents connection exhaustion under high load
    redis_manager: Option<Arc<redis::aio::ConnectionManager>>,
}

impl CacheExecutor {
    pub fn new() -> Self {
        Self {
            redis_manager: None,
        }
    }

    /// Create a new CacheExecutor with Redis connection
    pub async fn with_redis(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| Error::Internal(format!("Failed to create Redis client: {}", e)))?;

        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| {
                Error::Internal(format!("Failed to create Redis connection manager: {}", e))
            })?;

        Ok(Self {
            redis_manager: Some(Arc::new(manager)),
        })
    }

    /// Set Redis connection manager
    pub fn set_redis_manager(&mut self, manager: Arc<redis::aio::ConnectionManager>) {
        self.redis_manager = Some(manager);
    }
}

#[async_trait]
impl DataSourceExecutor for CacheExecutor {
    async fn execute(
        &self,
        _ctx: &Context,
        query: &DataSourceQuery,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        let start = Instant::now();

        // Extract cache key from query
        let key = match &query.source {
            DataSourceConfig::Cache { key } => {
                // Resolve template variables in key
                Self::resolve_cache_key(key, resolved_params)?
            }
            _ => {
                return Err(Error::Internal(
                    "Invalid source type for CacheExecutor".to_string(),
                ))
            }
        };

        tracing::debug!(
            source_id = %query.id,
            cache_key = %key,
            "Executing cache query"
        );

        // Get Redis connection
        let manager = self
            .redis_manager
            .as_ref()
            .ok_or_else(|| Error::Internal("Redis connection not configured".to_string()))?;

        let mut conn = (**manager).clone();

        // Execute GET command
        use redis::AsyncCommands;
        let cache_result: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| Error::Network(NetworkError::Request(format!("Redis error: {}", e))))?;

        let duration = start.elapsed();

        // Check if from cache before consuming cache_result
        let from_cache = cache_result.is_some();

        // Parse cached value as JSON
        let data = match cache_result {
            Some(ref value) => {
                // Try to parse as JSON
                serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.clone()))
            }
            None => {
                // Cache miss - return null instead of error
                tracing::debug!(
                    source_id = %query.id,
                    cache_key = %key,
                    "Cache miss"
                );
                serde_json::Value::Null
            }
        };

        // Warn if cache access was slow (should be <5ms)
        let mut warnings = vec![];
        if duration.as_millis() > 5 && from_cache {
            warnings.push(format!(
                "Cache access took {}ms (expected <5ms, possible Redis performance issue)",
                duration.as_millis()
            ));
        }

        Ok(DataSourceResult {
            source_id: query.id.clone(),
            data,
            metadata: ExecutionMetadata {
                duration_ms: duration.as_millis() as u64,
                from_cache,
                retries: 0,
                was_batched: false,
                warnings,
            },
        })
    }
}

impl CacheExecutor {
    /// Resolve template variables in cache key
    ///
    /// WHY: Cache keys often need dynamic values: "user:${userId}" or "session:${sessionId}"
    fn resolve_cache_key(
        key_template: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<String> {
        let mut result = key_template.to_string();

        // Replace ${variable} patterns
        for (param_key, value) in params {
            let placeholder = format!("${{{}}}", param_key);
            if result.contains(&placeholder) {
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue, // Skip complex values
                };
                result = result.replace(&placeholder, &replacement);
            }
        }

        // Also support nested paths: ${user.id}
        use regex::Regex;
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();

        for cap in re.captures_iter(&key_template.to_string()) {
            let full_match = cap.get(0).unwrap().as_str();
            let path = cap.get(1).unwrap().as_str();

            if let Ok(value) = Self::resolve_path(path, params) {
                let replacement = match value {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                result = result.replace(full_match, &replacement);
            }
        }

        Ok(result)
    }

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
