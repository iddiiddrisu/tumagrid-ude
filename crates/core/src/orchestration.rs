/*!
 * API Orchestration Models
 *
 * WHY THIS EXISTS:
 * ================
 * The real killer feature of SpaceForge is not just database access - it's the ability
 * to compose responses from multiple data sources (databases, REST APIs, GraphQL services,
 * serverless functions) in a SINGLE request with intelligent optimization.
 *
 * PROBLEM IT SOLVES:
 * ===================
 * In modern microservice architectures, frontends need to:
 * 1. Make 10+ API calls to render a single page (waterfall problem)
 * 2. Manually orchestrate dependencies (A must complete before B)
 * 3. Handle N+1 queries (fetching related data in loops)
 * 4. Deal with error handling across multiple services
 * 5. Write complex data merging logic
 *
 * EXAMPLE (E-commerce Homepage):
 * ===============================
 * Without SpaceForge (manual orchestration - 200+ lines of code):
 *   - Fetch user from Postgres
 *   - Fetch orders from MongoDB (depends on user)
 *   - For each order, call shipping API (N+1 problem!)
 *   - Fetch product details from GraphQL service
 *   - Call recommendation function
 *   - Manually merge all responses
 *   - Handle errors at each step
 *
 * With SpaceForge (declarative - 30 lines of config):
 *   - Define data sources and dependencies
 *   - SpaceForge handles parallelization, batching, merging, errors
 *   - One API call returns perfectly composed response
 *
 * COMPETITIVE ADVANTAGE:
 * ======================
 * - Hasura: Only database-to-GraphQL (can't compose with REST APIs)
 * - Apollo Federation: Requires complex subgraph setup for each service
 * - Spring Gateway/Zuul: Just routing, no data composition
 * - Kong/Nginx: Load balancing only, no intelligence
 *
 * SpaceForge: Netflix API Gateway on steroids - all data sources, zero code.
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

//═══════════════════════════════════════════════════════════════════════════
// COMPOSITE QUERY - The main query structure for orchestration
//═══════════════════════════════════════════════════════════════════════════

/// A composite query that fetches and combines data from multiple sources
///
/// WHY: Allows declaring complex data requirements in a single query instead of
/// making multiple sequential API calls from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeQuery {
    /// Unique identifier for this query
    pub id: String,

    /// List of data sources to fetch from
    /// WHY: Each source can be a different type (DB, API, function, etc.)
    /// and can depend on results from other sources
    pub sources: Vec<DataSourceQuery>,

    /// Template for composing the final response
    /// WHY: Allows transforming and merging data from different sources
    /// into the exact shape the client needs
    pub compose: CompositionTemplate,

    /// Optional caching strategy for the entire composed response
    /// WHY: Avoid re-executing expensive queries if data hasn't changed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheStrategy>,
}

//═══════════════════════════════════════════════════════════════════════════
// DATA SOURCE QUERY - Individual source in a composition
//═══════════════════════════════════════════════════════════════════════════

/// Represents a single data source in a composite query
///
/// WHY: Each source might be a different type (database, REST API, GraphQL, function)
/// and needs different configuration. This abstraction lets us treat them uniformly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceQuery {
    /// Unique ID for this source (used in dependencies and composition)
    pub id: String,

    /// The type and configuration of the data source
    pub source: DataSourceConfig,

    /// IDs of other sources this depends on
    /// WHY: Some data fetches require results from previous fetches
    /// Example: Can't fetch orders until we have the user ID
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Whether this can run in parallel with other sources
    /// WHY: Explicitly mark independent sources to maximize parallelization
    #[serde(default = "default_true")]
    pub parallel: bool,

    /// Optional caching for this specific source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheStrategy>,

    /// Optional timeout for this source (overrides global timeout)
    /// WHY: Some sources (like ML inference) might be slower than others
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Retry configuration for this source
    /// WHY: External APIs might be flaky, databases should rarely retry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
}

fn default_true() -> bool {
    true
}

//═══════════════════════════════════════════════════════════════════════════
// DATA SOURCE CONFIGURATIONS
//═══════════════════════════════════════════════════════════════════════════

/// Configuration for different types of data sources
///
/// WHY: Each source type has different parameters and capabilities.
/// This enum lets us handle all types uniformly while maintaining type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DataSourceConfig {
    /// Fetch from a database (uses existing CRUD module)
    Database {
        /// Database alias from config
        db_alias: String,

        /// Collection/table name
        collection: String,

        /// Query filter (can use template variables from previous sources)
        find: serde_json::Value,

        /// Query options (limit, sort, select, etc.)
        #[serde(default)]
        options: crate::ReadOptions,
    },

    /// Call a REST API (external/unmanaged)
    /// WHY: For external APIs (Stripe, Twilio) and legacy VMs not in our mesh
    /// NOTE: Use ServiceMesh instead for services we deployed and control
    RestApi {
        /// Base URL of the service
        url: String,

        /// HTTP method
        #[serde(default = "default_get")]
        method: String,

        /// Request headers (can use template variables)
        #[serde(default)]
        headers: HashMap<String, String>,

        /// Query parameters (can use template variables)
        #[serde(default)]
        params: HashMap<String, serde_json::Value>,

        /// Request body (for POST/PUT)
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,

        /// Whether this API supports batching multiple IDs
        /// WHY: Solves N+1 problem by batching requests like /products?ids=1,2,3
        #[serde(default)]
        batch: bool,

        /// Field in the request to use for batching
        #[serde(skip_serializing_if = "Option::is_none")]
        batch_field: Option<String>,
    },

    /// Call a service in the mesh (managed/orchestrated)
    /// WHY: For services WE deployed via runner - gets smart routing (health + latency)
    /// NOTE: Use RestApi for external APIs and services not in our mesh
    #[serde(rename = "servicemesh")]
    ServiceMesh {
        /// Service ID in the registry (e.g., "hubtel-payments")
        service: String,

        /// API path relative to service endpoint (e.g., "/v1/charge")
        path: String,

        /// HTTP method
        #[serde(default = "default_post")]
        method: String,

        /// Request headers (can use template variables)
        #[serde(default)]
        headers: HashMap<String, String>,

        /// Request body (can use template variables)
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,

        /// Routing preference (defaults to lowest-latency)
        #[serde(skip_serializing_if = "Option::is_none")]
        routing_preference: Option<RoutingStrategy>,
    },

    /// Call a GraphQL service
    /// WHY: Many services expose GraphQL, need to support cross-service composition
    GraphQL {
        /// GraphQL endpoint URL
        url: String,

        /// GraphQL query (can use template variables)
        query: String,

        /// Variables for the query
        #[serde(default)]
        variables: HashMap<String, serde_json::Value>,

        /// Request headers
        #[serde(default)]
        headers: HashMap<String, String>,
    },

    /// Call a serverless function
    /// WHY: Business logic often lives in functions, need to compose their results
    Function {
        /// Function name/ID
        name: String,

        /// Arguments to pass (can use template variables)
        #[serde(default)]
        args: HashMap<String, serde_json::Value>,
    },

    /// Fetch from cache
    /// WHY: Fast reads for frequently accessed data
    Cache {
        /// Cache key (can use template variables)
        key: String,
    },
}

fn default_get() -> String {
    "GET".to_string()
}

fn default_post() -> String {
    "POST".to_string()
}

//═══════════════════════════════════════════════════════════════════════════
// ROUTING STRATEGY
//═══════════════════════════════════════════════════════════════════════════

/// Strategy for routing requests in the service mesh
///
/// WHY: Different use cases need different routing strategies.
/// This is only used by ServiceMesh data sources (not RestApi).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Pick endpoint with lowest p50 latency (default)
    LowestLatency,

    /// Prefer a specific region, fall back to lowest latency
    RegionAffinity { preferred_region: String },

    /// Round-robin across healthy endpoints
    RoundRobin,

    /// Weighted distribution based on endpoint weights
    Weighted,

    /// Random selection (for testing)
    Random,
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        RoutingStrategy::LowestLatency
    }
}

//═══════════════════════════════════════════════════════════════════════════
// COMPOSITION TEMPLATE
//═══════════════════════════════════════════════════════════════════════════

/// Template for composing the final response from multiple sources
///
/// WHY: The client doesn't want raw results from each source - they want a
/// specific shape that might merge, transform, and filter data from all sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompositionTemplate {
    /// Simple template using template syntax
    /// Example: {"user": "${user}", "orders": "${orders}"}
    Template(serde_json::Value),

    /// Advanced composition with transformations
    Advanced {
        /// Fields in the output
        fields: HashMap<String, FieldTransform>,

        /// Optional post-processing filters
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<Vec<Filter>>,
    },
}

/// Transformation to apply to a field
///
/// WHY: Often need to transform data (map arrays, merge objects, extract fields)
/// before sending to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldTransform {
    /// Direct reference: "${source.field}"
    Reference(String),

    /// Map operation: "${source | map(x => x.field)}"
    Map {
        source: String,
        transform: String,
    },

    /// Merge operation: "${merge(source1, source2)}"
    Merge {
        sources: Vec<String>,
    },

    /// Filter operation: "${source | filter(x => x.active)}"
    Filter {
        source: String,
        condition: String,
    },

    /// Join operation: Merge results based on a key
    /// WHY: Core feature - join data from different sources like SQL JOIN
    /// Example: Join orders with shipping status by order ID
    Join {
        left: String,
        right: String,
        left_key: String,
        right_key: String,
        #[serde(default = "default_inner")]
        join_type: JoinType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Outer,
}

fn default_inner() -> JoinType {
    JoinType::Inner
}

/// Post-processing filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub operation: FilterOperation,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterOperation {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    In,
    NotIn,
    Contains,
}

//═══════════════════════════════════════════════════════════════════════════
// CACHING STRATEGIES
//═══════════════════════════════════════════════════════════════════════════

/// Caching strategy for queries
///
/// WHY: Different data has different freshness requirements. User profile can
/// be cached for minutes, but inventory needs to be fresh. This allows per-source
/// cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "lowercase")]
pub enum CacheStrategy {
    /// Time-to-live cache
    Ttl {
        /// Duration in seconds
        seconds: u64,
    },

    /// Stale-while-revalidate (serve stale, update in background)
    /// WHY: Better UX - instant response with stale data, fresh data next time
    StaleWhileRevalidate {
        stale_seconds: u64,
        revalidate_seconds: u64,
    },

    /// Invalidate on pattern
    /// WHY: Invalidate when related data changes (e.g., user cache when user updates)
    Invalidate {
        patterns: Vec<String>,
    },
}

//═══════════════════════════════════════════════════════════════════════════
// RETRY CONFIGURATION
//═══════════════════════════════════════════════════════════════════════════

/// Retry configuration for data sources
///
/// WHY: External APIs fail, networks are unreliable. Smart retries with backoff
/// improve reliability without overwhelming failing services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,

    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,

    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,

    /// Backoff multiplier (exponential backoff)
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_multiplier() -> f64 {
    2.0
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            multiplier: 2.0,
        }
    }
}

//═══════════════════════════════════════════════════════════════════════════
// EXECUTION PLAN
//═══════════════════════════════════════════════════════════════════════════

/// Execution plan generated by the query planner
///
/// WHY: The planner analyzes dependencies and creates an optimized execution plan
/// that maximizes parallelization while respecting data dependencies.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Stages to execute (each stage can run in parallel)
    /// WHY: Group independent queries into stages for parallel execution
    /// Example: Stage 1 = fetch user, Stage 2 = fetch orders+recommendations (parallel)
    pub stages: Vec<ExecutionStage>,

    /// Estimated execution time in milliseconds
    /// WHY: Help with timeout configuration and performance monitoring
    pub estimated_duration_ms: u64,
}

/// A single stage in the execution plan
///
/// WHY: Queries in a stage are independent and can run in parallel.
/// Stages must run sequentially because later stages depend on earlier ones.
#[derive(Debug, Clone)]
pub struct ExecutionStage {
    /// Queries to execute in parallel
    pub queries: Vec<String>, // IDs of DataSourceQuery

    /// Whether this stage can be cached as a unit
    pub cacheable: bool,
}

//═══════════════════════════════════════════════════════════════════════════
// EXECUTION RESULT
//═══════════════════════════════════════════════════════════════════════════

/// Result from executing a data source query
#[derive(Debug, Clone)]
pub struct DataSourceResult {
    /// ID of the source
    pub source_id: String,

    /// Result data
    pub data: serde_json::Value,

    /// Execution metadata
    pub metadata: ExecutionMetadata,
}

/// Metadata about query execution
///
/// WHY: Important for debugging, monitoring, and optimization. Shows what
/// actually happened during execution.
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Whether result came from cache
    pub from_cache: bool,

    /// Number of retries performed
    pub retries: u32,

    /// Whether this was batched with other queries
    pub was_batched: bool,

    /// Any warnings (e.g., "slow query", "rate limit approaching")
    pub warnings: Vec<String>,
}

/// Complete result from executing a composite query
///
/// WHY: Includes both the final data and execution metadata for
/// observability, debugging, and client information.
#[derive(Debug, Clone, Serialize)]
pub struct QueryExecutionResult {
    /// Final composed data
    pub data: serde_json::Value,

    /// Number of stages executed
    pub num_stages: usize,

    /// Number of data sources queried
    pub num_sources: usize,

    /// Whether any results came from cache
    pub used_cache: bool,

    /// Aggregated warnings from all sources
    pub warnings: Vec<String>,
}
