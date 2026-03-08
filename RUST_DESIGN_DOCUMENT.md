# UDE Rust Rewrite - Technical Design Document

## Executive Summary

This document outlines the complete technical design for rewriting UDE from Go to Rust. The design emphasizes **idiomatic Rust patterns** rather than direct code transposition, leveraging Rust's strengths in:
- Type safety and compile-time guarantees
- Memory safety without garbage collection
- Fearless concurrency with ownership system
- Zero-cost abstractions
- Rich trait system for polymorphism

**Target Architecture**: Multi-tenant, distributed Backend-as-a-Service platform with GraphQL/REST APIs, real-time subscriptions, event processing, and multi-database support.

---

## Table of Contents

1. [Core Architectural Principles](#1-core-architectural-principles)
2. [Crate Structure](#2-crate-structure)
3. [Trait System Design](#3-trait-system-design)
4. [Concurrency Model](#4-concurrency-model)
5. [Error Handling Strategy](#5-error-handling-strategy)
6. [Configuration Management](#6-configuration-management)
7. [Database Abstraction Layer](#7-database-abstraction-layer)
8. [HTTP/GraphQL Layer](#8-httpgraphql-layer)
9. [Authentication & Authorization](#9-authentication--authorization)
10. [Event System](#10-event-system)
11. [State Management](#11-state-management)
12. [Type Safety Enhancements](#12-type-safety-enhancements)
13. [Performance Optimizations](#13-performance-optimizations)
14. [Testing Strategy](#14-testing-strategy)
15. [Migration Path](#15-migration-path)

---

## 1. Core Architectural Principles

### 1.1 Rust Idioms vs Go Patterns

| Go Pattern | Rust Equivalent | Rationale |
|------------|-----------------|-----------|
| Interface-based DI | Trait objects (`dyn Trait`) + Generic bounds | More flexible, supports both static and dynamic dispatch |
| `sync.RWMutex` | `Arc<RwLock<T>>` or `Arc<Mutex<T>>` | Ownership-based safety, no data races |
| Goroutines | `tokio::spawn` + async/await | Structured concurrency, better error propagation |
| Channels (`chan`) | `tokio::sync::mpsc` or `crossbeam::channel` | Type-safe, bounded/unbounded options |
| `context.Context` | `tokio::time::timeout` + custom context struct | Explicit cancellation and deadlines |
| Error tuples `(T, error)` | `Result<T, Error>` | Compiler-enforced error handling |
| `nil` checks | `Option<T>` | Null safety at compile time |
| `interface{}` (any) | `Box<dyn Any>` or generics | Prefer generics for type safety |
| Init functions | Builder pattern + factory methods | Explicit initialization flow |
| Pointer receivers | `&self`, `&mut self`, or `self` | Ownership semantics prevent dangling references |

### 1.2 Key Design Goals

1. **Zero-Copy Where Possible**: Use references and slices instead of cloning
2. **Compile-Time Guarantees**: Leverage type system to prevent runtime errors
3. **Explicit Resource Management**: RAII for connections, files, locks
4. **Structured Concurrency**: Parent tasks manage child task lifetimes
5. **API Ergonomics**: Builder patterns, method chaining, sensible defaults
6. **Performance**: Target 2-3x throughput improvement over Go version

---

## 2. Crate Structure

### 2.1 Workspace Layout

```
space-cloud/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── gateway/                  # Main gateway binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs         # HTTP server setup
│   │       ├── routes.rs         # Route definitions
│   │       └── handlers/         # HTTP handlers
│   │           ├── mod.rs
│   │           ├── graphql.rs
│   │           ├── crud.rs
│   │           ├── eventing.rs
│   │           └── filestore.rs
│   │
│   ├── core/                     # Core domain logic (library)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs         # Configuration types
│   │       ├── error.rs          # Error types
│   │       ├── models.rs         # Domain models
│   │       └── traits.rs         # Core trait definitions
│   │
│   ├── managers/                 # Cluster-level coordination (library)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── admin.rs          # Admin manager
│   │       ├── sync/             # Sync manager
│   │       │   ├── mod.rs
│   │       │   ├── config_sync.rs
│   │       │   ├── store.rs
│   │       │   └── pubsub.rs
│   │       └── integration.rs    # Integration hooks
│   │
│   ├── modules/                  # Per-project modules (library)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── auth/             # Authentication & authorization
│   │       │   ├── mod.rs
│   │       │   ├── jwt.rs
│   │       │   ├── rules.rs
│   │       │   └── postprocess.rs
│   │       ├── crud/             # Database operations
│   │       │   ├── mod.rs
│   │       │   ├── interface.rs  # Crud trait
│   │       │   ├── sql.rs        # SQL driver
│   │       │   ├── mongodb.rs    # MongoDB driver
│   │       │   ├── query.rs      # Query builder
│   │       │   └── dataloader.rs # Batch optimization
│   │       ├── schema/           # Schema management
│   │       │   ├── mod.rs
│   │       │   ├── validation.rs
│   │       │   └── inspector.rs
│   │       ├── eventing/         # Event system
│   │       │   ├── mod.rs
│   │       │   ├── processor.rs
│   │       │   ├── webhook.rs
│   │       │   └── queue.rs
│   │       ├── filestore/        # File storage
│   │       │   ├── mod.rs
│   │       │   ├── local.rs
│   │       │   ├── s3.rs
│   │       │   └── gcs.rs
│   │       ├── functions/        # Service invocation
│   │       │   ├── mod.rs
│   │       │   └── templating.rs
│   │       ├── realtime/         # WebSocket subscriptions
│   │       │   ├── mod.rs
│   │       │   ├── subscriptions.rs
│   │       │   └── feed.rs
│   │       └── graphql/          # GraphQL engine
│   │           ├── mod.rs
│   │           ├── schema.rs
│   │           ├── executor.rs
│   │           └── resolvers.rs
│   │
│   ├── utils/                    # Shared utilities (library)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pubsub.rs         # Redis pub/sub
│   │       ├── metrics.rs        # Telemetry
│   │       ├── http_client.rs    # HTTP utilities
│   │       └── template.rs       # Template engine
│   │
│   ├── runner/                   # Runner service (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   │
│   └── cli/                      # CLI tool (binary)
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
│
├── tests/                        # Integration tests
│   ├── common/
│   │   └── mod.rs
│   ├── crud_tests.rs
│   ├── graphql_tests.rs
│   └── eventing_tests.rs
│
└── benches/                      # Benchmarks
    ├── query_performance.rs
    └── event_processing.rs
```

### 2.2 Key Dependencies

```toml
# crates/gateway/Cargo.toml
[dependencies]
# Async runtime
tokio = { version = "1.40", features = ["full"] }

# HTTP framework
axum = { version = "0.7", features = ["ws", "multipart"] }
tower = { version = "0.5", features = ["timeout", "limit", "buffer"] }
tower-http = { version = "0.6", features = ["cors", "trace", "compression"] }

# GraphQL
async-graphql = { version = "7.0", features = ["dataloader"] }
async-graphql-axum = "7.0"

# Database drivers
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "mysql", "mssql", "json"] }
mongodb = "3.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"

# Authentication
jsonwebtoken = "9.3"

# Redis
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }

# HTTP client
reqwest = { version = "0.12", features = ["json"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Templating
tera = "1.20"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Configuration
config = "0.14"

# Concurrency
arc-swap = "1.7"
dashmap = "6.1"
crossbeam = "0.8"

# File storage
aws-sdk-s3 = "1.54"
```

---

## 3. Trait System Design

### 3.1 Core Trait Hierarchy

Replace Go's 30+ interfaces with a coherent trait hierarchy:

```rust
// crates/core/src/traits.rs

use async_trait::async_trait;
use std::sync::Arc;
use crate::{error::Result, models::*};

//═══════════════════════════════════════════════════════════
// CRUD TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait CrudOperations: Send + Sync {
    async fn create(&self, ctx: &Context, col: &str, req: CreateRequest)
        -> Result<u64>;

    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest)
        -> Result<ReadResponse>;

    async fn update(&self, ctx: &Context, col: &str, req: UpdateRequest)
        -> Result<u64>;

    async fn delete(&self, ctx: &Context, col: &str, req: DeleteRequest)
        -> Result<u64>;

    async fn aggregate(&self, ctx: &Context, col: &str, req: AggregateRequest)
        -> Result<serde_json::Value>;

    async fn batch(&self, ctx: &Context, req: BatchRequest)
        -> Result<Vec<u64>>;

    async fn describe_table(&self, ctx: &Context, col: &str)
        -> Result<TableDescription>;

    async fn raw_query(&self, ctx: &Context, query: &str, args: Vec<Value>)
        -> Result<RawQueryResponse>;

    fn get_db_type(&self) -> DbType;
    fn is_connected(&self) -> bool;
}

// Specialized traits for different module interactions
#[async_trait]
pub trait CrudForAuth: Send + Sync {
    async fn read(&self, ctx: &Context, db_alias: &str, col: &str,
                  req: ReadRequest, params: RequestParams)
        -> Result<ReadResponse>;
}

#[async_trait]
pub trait CrudForEventing: Send + Sync {
    async fn internal_create(&self, ctx: &Context, db_alias: &str,
                            project: &str, col: &str, req: CreateRequest)
        -> Result<()>;

    async fn internal_update(&self, ctx: &Context, db_alias: &str,
                            project: &str, col: &str, req: UpdateRequest)
        -> Result<()>;
}

#[async_trait]
pub trait CrudForSchema: Send + Sync {
    async fn get_db_type(&self, db_alias: &str) -> Result<DbType>;
    async fn raw_batch(&self, ctx: &Context, db_alias: &str,
                      queries: Vec<String>) -> Result<()>;
    async fn describe_table(&self, ctx: &Context, db_alias: &str, col: &str)
        -> Result<TableDescription>;
}

//═══════════════════════════════════════════════════════════
// AUTH TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait AuthOperations: Send + Sync {
    async fn parse_token(&self, ctx: &Context, token: &str)
        -> Result<TokenClaims>;

    async fn create_token(&self, ctx: &Context, claims: TokenClaims)
        -> Result<String>;

    async fn is_read_authorized(&self, ctx: &Context, project: &str,
                               db_type: DbType, col: &str, token: &str,
                               req: &ReadRequest)
        -> Result<(PostProcess, RequestParams)>;

    async fn is_create_authorized(&self, ctx: &Context, project: &str,
                                  db_type: DbType, col: &str, token: &str,
                                  req: &CreateRequest)
        -> Result<RequestParams>;

    async fn is_update_authorized(&self, ctx: &Context, project: &str,
                                  db_type: DbType, col: &str, token: &str,
                                  req: &UpdateRequest)
        -> Result<RequestParams>;

    async fn is_delete_authorized(&self, ctx: &Context, project: &str,
                                  db_type: DbType, col: &str, token: &str,
                                  req: &DeleteRequest)
        -> Result<RequestParams>;

    async fn post_process(&self, ctx: &Context, pp: PostProcess,
                         result: &mut serde_json::Value) -> Result<()>;
}

#[async_trait]
pub trait AuthForEventing: Send + Sync {
    async fn get_internal_token(&self, ctx: &Context) -> Result<String>;
    async fn is_eventing_authorized(&self, ctx: &Context, project: &str,
                                   token: &str, event: &QueueEventRequest)
        -> Result<RequestParams>;
}

//═══════════════════════════════════════════════════════════
// SCHEMA TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait SchemaOperations: Send + Sync {
    async fn set_schema(&self, db_alias: &str, schemas: DatabaseSchemas)
        -> Result<()>;

    async fn validate_create(&self, ctx: &Context, db_type: DbType,
                            col: &str, req: &CreateRequest) -> Result<()>;

    async fn validate_update(&self, ctx: &Context, db_type: DbType,
                            col: &str, op: &str,
                            update_doc: &Value, find: &Value) -> Result<()>;

    async fn post_process(&self, ctx: &Context, db_alias: &str, col: &str,
                         result: &mut serde_json::Value) -> Result<()>;

    fn get_schema(&self, db_alias: &str, col: &str) -> Option<Fields>;
}

//═══════════════════════════════════════════════════════════
// EVENTING TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait EventingOperations: Send + Sync {
    async fn create_intent_hook(&self, ctx: &Context, req: &EventIntent)
        -> Result<String>;

    async fn hook_stage(&self, ctx: &Context, intent_id: &str,
                       error: Option<&str>) -> Result<()>;

    async fn queue_event(&self, ctx: &Context, req: QueueEventRequest)
        -> Result<()>;
}

#[async_trait]
pub trait EventingForRealtime: Send + Sync {
    async fn subscribe_to_events(&self, ctx: &Context, filter: EventFilter)
        -> Result<EventStream>;
}

//═══════════════════════════════════════════════════════════
// FILESTORE TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait FileStoreOperations: Send + Sync {
    async fn upload(&self, ctx: &Context, req: UploadRequest) -> Result<()>;
    async fn download(&self, ctx: &Context, path: &str) -> Result<FileStream>;
    async fn delete(&self, ctx: &Context, path: &str) -> Result<()>;
    async fn list(&self, ctx: &Context, prefix: &str) -> Result<Vec<FileInfo>>;
}

//═══════════════════════════════════════════════════════════
// SYNC MANAGER TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn load_config(&self) -> Result<Config>;
    async fn save_config(&self, config: &Config) -> Result<()>;
    async fn watch_config(&self) -> Result<ConfigWatchStream>;
}

#[async_trait]
pub trait SyncOperations: Send + Sync {
    async fn set_database_config(&self, project_id: &str,
                                db_alias: &str, config: DatabaseConfig)
        -> Result<()>;

    async fn set_eventing_config(&self, project_id: &str,
                                config: EventingConfig) -> Result<()>;

    async fn get_assigned_space_cloud_id(&self, ctx: &Context,
                                        project: &str, token: i32)
        -> Result<String>;

    fn get_node_id(&self) -> &str;
    fn is_leader(&self) -> bool;
}
```

### 3.2 Trait Object vs Generic Bounds

**Use trait objects (`Box<dyn Trait>`, `Arc<dyn Trait>`) when:**
- Runtime polymorphism is needed (different database drivers)
- Heterogeneous collections required
- Plugin/extension architecture

**Use generic bounds (`<T: Trait>`) when:**
- Static dispatch for performance
- Monomorphization acceptable
- Single concrete type per instantiation

```rust
// Trait object for runtime polymorphism
pub struct CrudModule {
    drivers: HashMap<String, Arc<dyn CrudOperations>>,
}

// Generic bounds for static dispatch
pub async fn execute_query<C: CrudOperations>(
    crud: &C,
    ctx: &Context,
    query: ReadRequest,
) -> Result<ReadResponse> {
    crud.read(ctx, "users", query).await
}
```

---

## 4. Concurrency Model

### 4.1 State Sharing Patterns

Replace Go's `sync.RWMutex` with Rust's ownership-based concurrency:

```rust
// crates/modules/src/auth/mod.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use arc_swap::ArcSwap; // For lock-free reads

pub struct AuthModule {
    // Immutable fields (no lock needed)
    cluster_id: Arc<str>,
    node_id: Arc<str>,

    // Infrequently updated config (arc-swap for lock-free reads)
    config: Arc<ArcSwap<AuthConfig>>,

    // Mutable state requiring coordination
    db_rules: Arc<RwLock<HashMap<String, DatabaseRule>>>,

    // Dependencies (trait objects behind Arc)
    crud: Arc<dyn CrudForAuth>,
    admin_manager: Arc<dyn AdminOperations>,
    jwt_handler: JwtHandler, // Owned, immutable
}

impl AuthModule {
    pub fn new(
        cluster_id: String,
        node_id: String,
        crud: Arc<dyn CrudForAuth>,
        admin_manager: Arc<dyn AdminOperations>,
    ) -> Self {
        Self {
            cluster_id: Arc::from(cluster_id),
            node_id: Arc::from(node_id),
            config: Arc::new(ArcSwap::from_pointee(AuthConfig::default())),
            db_rules: Arc::new(RwLock::new(HashMap::new())),
            crud,
            admin_manager,
            jwt_handler: JwtHandler::new(),
        }
    }

    // Lock-free read of config
    pub fn get_config(&self) -> Arc<AuthConfig> {
        self.config.load_full()
    }

    // Infrequent write with atomic swap
    pub async fn set_config(&self, config: AuthConfig) {
        self.config.store(Arc::new(config));
    }

    // Read with RwLock
    pub async fn get_rule(&self, collection: &str) -> Option<DatabaseRule> {
        let rules = self.db_rules.read().await;
        rules.get(collection).cloned()
    }

    // Write with RwLock
    pub async fn set_rule(&self, collection: String, rule: DatabaseRule) {
        let mut rules = self.db_rules.write().await;
        rules.insert(collection, rule);
    }
}
```

### 4.2 Async/Await Patterns

Replace goroutines with structured concurrency:

```rust
// crates/modules/src/eventing/processor.rs

use tokio::{select, task::JoinHandle};
use tokio::sync::{mpsc, broadcast};
use tokio::time::{interval, Duration};

pub struct EventProcessor {
    config: Arc<EventingConfig>,
    crud: Arc<dyn CrudForEventing>,

    // Channels for inter-task communication
    intent_tx: mpsc::Sender<EventIntent>,
    staged_tx: mpsc::Sender<StagedEvent>,
    shutdown_tx: broadcast::Sender<()>,

    // Task handles for lifecycle management
    tasks: Vec<JoinHandle<Result<()>>>,
}

impl EventProcessor {
    pub fn new(
        config: Arc<EventingConfig>,
        crud: Arc<dyn CrudForEventing>,
    ) -> Self {
        let (intent_tx, _) = mpsc::channel(1000);
        let (staged_tx, _) = mpsc::channel(1000);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config,
            crud,
            intent_tx,
            staged_tx,
            shutdown_tx,
            tasks: Vec::new(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        // Start intent processing task
        let handle = tokio::spawn(
            Self::process_intents_loop(
                self.intent_tx.subscribe(),
                self.staged_tx.clone(),
                self.crud.clone(),
                self.shutdown_tx.subscribe(),
            )
        );
        self.tasks.push(handle);

        // Start staged event processing task
        let handle = tokio::spawn(
            Self::process_staged_loop(
                self.staged_tx.subscribe(),
                self.config.clone(),
                self.crud.clone(),
                self.shutdown_tx.subscribe(),
            )
        );
        self.tasks.push(handle);

        Ok(())
    }

    async fn process_intents_loop(
        mut intent_rx: mpsc::Receiver<EventIntent>,
        staged_tx: mpsc::Sender<StagedEvent>,
        crud: Arc<dyn CrudForEventing>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(10));

        loop {
            select! {
                Some(intent) = intent_rx.recv() => {
                    // Process intent
                    match Self::process_intent(&crud, intent).await {
                        Ok(staged) => {
                            let _ = staged_tx.send(staged).await;
                        }
                        Err(e) => {
                            tracing::error!("Failed to process intent: {}", e);
                        }
                    }
                }
                _ = ticker.tick() => {
                    // Periodic polling for orphaned intents
                    Self::poll_intents(&crud, &staged_tx).await?;
                }
                _ = shutdown.recv() => {
                    tracing::info!("Shutting down intent processor");
                    break Ok(());
                }
            }
        }
    }

    pub async fn shutdown(self) -> Result<()> {
        // Signal all tasks to shut down
        let _ = self.shutdown_tx.send(());

        // Wait for all tasks to complete
        for handle in self.tasks {
            handle.await??;
        }

        Ok(())
    }
}
```

### 4.3 Context Propagation

Replace Go's `context.Context` with explicit context passing:

```rust
// crates/core/src/models.rs

use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Context {
    pub request_id: String,
    pub timeout: Option<Duration>,
    pub claims: Option<TokenClaims>,
    pub metadata: HashMap<String, String>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            timeout: Some(Duration::from_secs(30)),
            claims: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_claims(mut self, claims: TokenClaims) -> Self {
        self.claims = Some(claims);
        self
    }
}

// Usage in async functions
pub async fn execute_with_timeout<F, T>(
    ctx: &Context,
    future: F,
) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    match ctx.timeout {
        Some(timeout) => {
            tokio::time::timeout(timeout, future)
                .await
                .map_err(|_| Error::Timeout)?
        }
        None => future.await,
    }
}
```

---

## 5. Error Handling Strategy

### 5.1 Error Type Hierarchy

Use `thiserror` for structured error types:

```rust
// crates/core/src/error.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Infrastructure errors
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    // Domain errors
    #[error("Authentication failed: {0}")]
    Auth(#[from] AuthError),

    #[error("Authorization denied: {reason}")]
    Unauthorized { reason: String },

    #[error("Validation failed: {field}: {message}")]
    Validation { field: String, message: String },

    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound { resource_type: String, id: String },

    // Operational errors
    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Internal server error: {0}")]
    Internal(String),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Query failed: {0}")]
    Query(String),

    #[error("Transaction failed: {0}")]
    Transaction(String),

    #[error("Constraint violation: {0}")]
    Constraint(String),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Missing required claim: {0}")]
    MissingClaim(String),
}

// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

// HTTP error responses
impl Error {
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Auth(_) => 401,
            Error::Unauthorized { .. } => 403,
            Error::NotFound { .. } => 404,
            Error::Validation { .. } => 400,
            Error::RateLimit => 429,
            Error::Timeout(_) => 504,
            _ => 500,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Error::Database(_) => "DB_ERROR",
            Error::Auth(_) => "AUTH_ERROR",
            Error::Unauthorized { .. } => "UNAUTHORIZED",
            Error::Validation { .. } => "VALIDATION_ERROR",
            Error::NotFound { .. } => "NOT_FOUND",
            Error::Timeout(_) => "TIMEOUT",
            Error::RateLimit => "RATE_LIMIT",
            _ => "INTERNAL_ERROR",
        }
    }
}

// Axum integration
impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.status_code())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        let body = serde_json::json!({
            "error": {
                "code": self.error_code(),
                "message": self.to_string(),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}
```

### 5.2 Error Context and Tracing

```rust
use tracing::{error, warn, info};

pub async fn process_request(ctx: &Context, req: Request) -> Result<Response> {
    info!(
        request_id = %ctx.request_id,
        endpoint = %req.endpoint,
        "Processing request"
    );

    let result = perform_operation(&req)
        .await
        .map_err(|e| {
            error!(
                request_id = %ctx.request_id,
                error = %e,
                "Operation failed"
            );
            e
        })?;

    info!(
        request_id = %ctx.request_id,
        "Request processed successfully"
    );

    Ok(result)
}
```

---

## 6. Configuration Management

### 6.1 Configuration Types

```rust
// crates/core/src/config.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub projects: HashMap<String, Project>,
    pub ssl: Option<SslConfig>,
    pub cluster_config: ClusterConfig,
    pub integrations: HashMap<String, Integration>,
    pub cache_config: Option<CacheConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub project_config: ProjectConfig,
    pub database_configs: HashMap<String, DatabaseConfig>,
    pub database_schemas: HashMap<String, DatabaseSchema>,
    pub database_rules: HashMap<String, DatabaseRule>,
    pub eventing_config: Option<EventingConfig>,
    pub eventing_triggers: HashMap<String, EventingTrigger>,
    pub filestore_config: Option<FileStoreConfig>,
    pub filestore_rules: HashMap<String, FileRule>,
    pub auths: HashMap<String, AuthConfig>,
    pub ingress_routes: HashMap<String, IngressRoute>,
    pub remote_services: HashMap<String, RemoteService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    #[serde(default)]
    pub aes_key: String,
    #[serde(default = "default_context_time")]
    pub context_time_graphql: u64, // milliseconds
}

fn default_context_time() -> u64 {
    30000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DbType {
    Postgres,
    Mysql,
    Sqlserver,
    Mongo,
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub id: String,
    pub db_alias: String,
    #[serde(rename = "type")]
    pub db_type: DbType,
    pub enabled: bool,
    pub conn: String,
    pub name: String,
    #[serde(default)]
    pub driver_config: DriverConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverConfig {
    #[serde(default = "default_max_conn")]
    pub max_conn: u32,
    #[serde(default = "default_max_idle_timeout")]
    pub max_idle_timeout: u64, // seconds
    #[serde(default)]
    pub min_conn: u32,
    #[serde(default)]
    pub max_idle_conn: u32,
}

fn default_max_conn() -> u32 { 50 }
fn default_max_idle_timeout() -> u64 { 300 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseRule {
    pub id: String,
    pub db_alias: String,
    pub col: String,
    pub rules: Rules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rules {
    pub create: Option<Rule>,
    pub read: Option<Rule>,
    pub update: Option<Rule>,
    pub delete: Option<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "lowercase")]
pub enum Rule {
    Allow,
    Deny,
    Authenticated,
    Match {
        #[serde(rename = "type")]
        match_type: MatchType,
        f1: Value,
        f2: Value,
    },
    And {
        clauses: Vec<Rule>,
    },
    Or {
        clauses: Vec<Rule>,
    },
    Query {
        db_alias: String,
        col: String,
        find: Value,
    },
    Webhook {
        url: String,
        #[serde(default)]
        template: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = "<=")]
    LessThanOrEqual,
    In,
    NotIn,
    Contains,
}
```

### 6.2 Configuration Loading

```rust
// crates/managers/src/sync/config_loader.rs

use config::{Config as ConfigBuilder, File, FileFormat, Environment};

pub struct ConfigLoader;

impl ConfigLoader {
    pub async fn load_from_file(path: &str) -> Result<Config> {
        let builder = ConfigBuilder::builder()
            .add_source(File::new(path, FileFormat::Yaml))
            .add_source(Environment::with_prefix("SC"))
            .build()?;

        let config: Config = builder.try_deserialize()?;
        Self::validate(&config)?;
        Ok(config)
    }

    fn validate(config: &Config) -> Result<()> {
        // Validate cluster config
        if config.cluster_config.letsencrypt_email.is_empty() {
            return Err(Error::Config(
                ConfigError::MissingField("letsencrypt_email".into())
            ));
        }

        // Validate each project
        for (project_id, project) in &config.projects {
            if project.project_config.id != *project_id {
                return Err(Error::Config(
                    ConfigError::InvalidField {
                        field: "id".into(),
                        message: "Project ID mismatch".into(),
                    }
                ));
            }

            // Validate database configs
            for (alias, db_config) in &project.database_configs {
                if db_config.conn.is_empty() {
                    return Err(Error::Config(
                        ConfigError::MissingField(
                            format!("projects.{}.database_configs.{}.conn",
                                    project_id, alias)
                        )
                    ));
                }
            }
        }

        Ok(())
    }
}
```

### 6.3 Hot Reloading with arc-swap

```rust
// crates/managers/src/sync/config_sync.rs

use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::watch;

pub struct ConfigManager {
    config: Arc<ArcSwap<Config>>,
    update_tx: watch::Sender<()>,
    update_rx: watch::Receiver<()>,
}

impl ConfigManager {
    pub fn new(initial_config: Config) -> Self {
        let (tx, rx) = watch::channel(());
        Self {
            config: Arc::new(ArcSwap::from_pointee(initial_config)),
            update_tx: tx,
            update_rx: rx,
        }
    }

    // Lock-free read
    pub fn get(&self) -> Arc<Config> {
        self.config.load_full()
    }

    // Atomic update
    pub async fn update(&self, new_config: Config) -> Result<()> {
        // Validate before swapping
        Self::validate_config(&new_config)?;

        // Atomic swap
        self.config.store(Arc::new(new_config));

        // Notify watchers
        let _ = self.update_tx.send(());

        Ok(())
    }

    // Watch for updates
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.update_rx.clone()
    }
}
```

---

## 7. Database Abstraction Layer

### 7.1 Unified Query Model

```rust
// crates/modules/src/crud/interface.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadRequest {
    pub find: Value,
    pub options: ReadOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadOptions {
    #[serde(default)]
    pub select: Option<Value>,
    #[serde(default)]
    pub sort: Vec<String>,
    #[serde(default)]
    pub skip: u64,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub distinct: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReadResponse {
    pub count: u64,
    pub data: Vec<Value>,
    pub metadata: Option<SqlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    pub op: CreateOp,
    pub doc: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreateOp {
    One,
    All,
    Upsert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub find: Value,
    pub update: Value,
    pub op: UpdateOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateOp {
    Set,
    Inc,
    Dec,
    Mul,
    Push,
    Rename,
    Unset,
}
```

### 7.2 SQL Driver Implementation

```rust
// crates/modules/src/crud/sql.rs

use sqlx::{Pool, Postgres, Row, FromRow};
use sqlx::postgres::{PgPoolOptions, PgConnectOptions};

pub struct SqlDriver {
    pool: Pool<Postgres>,
    db_type: DbType,
    db_name: String,
}

impl SqlDriver {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let options = config.conn.parse::<PgConnectOptions>()?
            .database(&config.name);

        let pool = PgPoolOptions::new()
            .max_connections(config.driver_config.max_conn)
            .min_connections(config.driver_config.min_conn)
            .idle_timeout(Duration::from_secs(
                config.driver_config.max_idle_timeout
            ))
            .connect_with(options)
            .await?;

        Ok(Self {
            pool,
            db_type: config.db_type.clone(),
            db_name: config.name.clone(),
        })
    }
}

#[async_trait]
impl CrudOperations for SqlDriver {
    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest)
        -> Result<ReadResponse> {
        // Build SQL query
        let (sql, params) = self.build_select_query(col, &req)?;

        tracing::debug!(
            request_id = %ctx.request_id,
            sql = %sql,
            "Executing SQL query"
        );

        // Execute with timeout
        let rows = execute_with_timeout(ctx, async {
            sqlx::query(&sql)
                .bind_all(params)
                .fetch_all(&self.pool)
                .await
        })
        .await?;

        // Convert rows to JSON
        let data: Vec<Value> = rows.into_iter()
            .map(|row| row_to_json(&row))
            .collect::<Result<_>>()?;

        Ok(ReadResponse {
            count: data.len() as u64,
            data,
            metadata: None,
        })
    }

    async fn create(&self, ctx: &Context, col: &str, req: CreateRequest)
        -> Result<u64> {
        let (sql, params) = self.build_insert_query(col, &req)?;

        let result = execute_with_timeout(ctx, async {
            sqlx::query(&sql)
                .bind_all(params)
                .execute(&self.pool)
                .await
        })
        .await?;

        Ok(result.rows_affected())
    }

    // ... other methods

    fn get_db_type(&self) -> DbType {
        self.db_type.clone()
    }

    fn is_connected(&self) -> bool {
        !self.pool.is_closed()
    }
}

impl SqlDriver {
    fn build_select_query(&self, col: &str, req: &ReadRequest)
        -> Result<(String, Vec<Value>)> {
        let mut query = String::from("SELECT ");

        // SELECT clause
        match &req.options.select {
            Some(select) => {
                let fields = self.parse_select(select)?;
                query.push_str(&fields.join(", "));
            }
            None => query.push('*'),
        }

        query.push_str(&format!(" FROM {}", col));

        // WHERE clause
        let (where_clause, params) = self.build_where_clause(&req.find)?;
        if !where_clause.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_clause);
        }

        // ORDER BY clause
        if !req.options.sort.is_empty() {
            query.push_str(" ORDER BY ");
            let sort_fields: Vec<String> = req.options.sort.iter()
                .map(|s| {
                    if s.starts_with('-') {
                        format!("{} DESC", &s[1..])
                    } else {
                        format!("{} ASC", s)
                    }
                })
                .collect();
            query.push_str(&sort_fields.join(", "));
        }

        // LIMIT/OFFSET
        if let Some(limit) = req.options.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        if req.options.skip > 0 {
            query.push_str(&format!(" OFFSET {}", req.options.skip));
        }

        Ok((query, params))
    }

    fn build_where_clause(&self, find: &Value)
        -> Result<(String, Vec<Value>)> {
        match find {
            Value::Object(map) => {
                let mut clauses = Vec::new();
                let mut params = Vec::new();
                let mut param_idx = 1;

                for (key, value) in map {
                    match value {
                        Value::Object(op_map) => {
                            // Operator query: {age: {">": 18}}
                            for (op, val) in op_map {
                                let sql_op = match op.as_str() {
                                    ">" => ">",
                                    ">=" => ">=",
                                    "<" => "<",
                                    "<=" => "<=",
                                    "!=" => "!=",
                                    "in" => "IN",
                                    "notIn" => "NOT IN",
                                    _ => return Err(Error::Validation {
                                        field: key.clone(),
                                        message: format!("Unknown operator: {}", op),
                                    }),
                                };

                                clauses.push(format!("{} {} ${}",
                                                    key, sql_op, param_idx));
                                params.push(val.clone());
                                param_idx += 1;
                            }
                        }
                        _ => {
                            // Equality: {name: "John"}
                            clauses.push(format!("{} = ${}", key, param_idx));
                            params.push(value.clone());
                            param_idx += 1;
                        }
                    }
                }

                Ok((clauses.join(" AND "), params))
            }
            _ => Err(Error::Validation {
                field: "find".into(),
                message: "Find clause must be an object".into(),
            }),
        }
    }
}
```

### 7.3 MongoDB Driver

```rust
// crates/modules/src/crud/mongodb.rs

use mongodb::{Client, Collection, options::*};
use mongodb::bson::{doc, Document, to_bson};

pub struct MongoDriver {
    client: Client,
    db_name: String,
}

impl MongoDriver {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let options = ClientOptions::parse(&config.conn).await?
            .with_max_pool_size(Some(config.driver_config.max_conn))
            .with_min_pool_size(Some(config.driver_config.min_conn as u32));

        let client = Client::with_options(options)?;

        Ok(Self {
            client,
            db_name: config.name.clone(),
        })
    }

    fn collection(&self, name: &str) -> Collection<Document> {
        self.client.database(&self.db_name).collection(name)
    }
}

#[async_trait]
impl CrudOperations for MongoDriver {
    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest)
        -> Result<ReadResponse> {
        let collection = self.collection(col);

        // Convert filter
        let filter = json_to_bson(&req.find)?;

        // Build find options
        let mut options = FindOptions::default();
        if let Some(select) = &req.options.select {
            options.projection = Some(json_to_bson(select)?);
        }
        if !req.options.sort.is_empty() {
            options.sort = Some(self.build_sort(&req.options.sort)?);
        }
        options.skip = Some(req.options.skip);
        options.limit = req.options.limit.map(|l| l as i64);

        // Execute query
        let cursor = execute_with_timeout(ctx, async {
            collection.find(filter, options).await
        })
        .await?;

        // Collect results
        let documents: Vec<Document> = cursor.try_collect().await?;
        let data: Vec<Value> = documents.into_iter()
            .map(|doc| bson_to_json(&doc))
            .collect::<Result<_>>()?;

        Ok(ReadResponse {
            count: data.len() as u64,
            data,
            metadata: None,
        })
    }

    // ... other methods
}
```

### 7.4 Driver Registry

```rust
// crates/modules/src/crud/mod.rs

pub struct CrudModule {
    drivers: Arc<RwLock<HashMap<String, Arc<dyn CrudOperations>>>>,
    schema: Arc<dyn SchemaOperations>,
    auth: Arc<dyn AuthCrudInterface>,
}

impl CrudModule {
    pub async fn new(
        config: &HashMap<String, DatabaseConfig>,
        schema: Arc<dyn SchemaOperations>,
        auth: Arc<dyn AuthCrudInterface>,
    ) -> Result<Self> {
        let mut drivers = HashMap::new();

        for (alias, db_config) in config {
            if !db_config.enabled {
                continue;
            }

            let driver: Arc<dyn CrudOperations> = match db_config.db_type {
                DbType::Postgres | DbType::Mysql | DbType::Sqlserver => {
                    Arc::new(SqlDriver::new(db_config).await?)
                }
                DbType::Mongo => {
                    Arc::new(MongoDriver::new(db_config).await?)
                }
                DbType::Embedded => {
                    Arc::new(BoltDriver::new(db_config).await?)
                }
            };

            drivers.insert(alias.clone(), driver);
        }

        Ok(Self {
            drivers: Arc::new(RwLock::new(drivers)),
            schema,
            auth,
        })
    }

    pub async fn get_driver(&self, db_alias: &str)
        -> Result<Arc<dyn CrudOperations>> {
        let drivers = self.drivers.read().await;
        drivers.get(db_alias)
            .cloned()
            .ok_or_else(|| Error::NotFound {
                resource_type: "database".into(),
                id: db_alias.into(),
            })
    }

    pub async fn read(&self, ctx: &Context, db_alias: &str, col: &str,
                     req: ReadRequest, params: RequestParams)
        -> Result<ReadResponse> {
        // Get driver
        let driver = self.get_driver(db_alias).await?;

        // Validate schema
        self.schema.validate_read(ctx, driver.get_db_type(), col, &req)
            .await?;

        // Execute query
        let mut response = driver.read(ctx, col, req).await?;

        // Post-process (schema transformations)
        for value in &mut response.data {
            self.schema.post_process(ctx, db_alias, col, value).await?;
        }

        Ok(response)
    }
}
```

---

## 8. HTTP/GraphQL Layer

### 8.1 Axum Server Setup

```rust
// crates/gateway/src/server.rs

use axum::{
    Router,
    routing::{get, post},
    middleware,
    extract::State,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{CorsLayer, Any},
    trace::TraceLayer,
    compression::CompressionLayer,
    timeout::TimeoutLayer,
};
use std::time::Duration;

pub struct Server {
    app_state: Arc<AppState>,
    port: u16,
}

#[derive(Clone)]
pub struct AppState {
    pub managers: Arc<Managers>,
    pub modules: Arc<RwLock<HashMap<String, ProjectModules>>>,
    pub config: Arc<ArcSwap<Config>>,
}

impl Server {
    pub fn new(
        managers: Arc<Managers>,
        config: Arc<ArcSwap<Config>>,
    ) -> Self {
        let app_state = Arc::new(AppState {
            managers,
            modules: Arc::new(RwLock::new(HashMap::new())),
            config,
        });

        Self {
            app_state,
            port: 4122,
        }
    }

    pub async fn start(self) -> Result<()> {
        let app = self.build_router();

        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], self.port));

        tracing::info!("Starting server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .await
            .map_err(|e| Error::Network(NetworkError::ServerError(e.to_string())))?;

        Ok(())
    }

    fn build_router(&self) -> Router {
        // Middleware stack
        let middleware_stack = ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(TimeoutLayer::new(Duration::from_secs(30)))
            .layer(CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any));

        // API routes
        let api_routes = Router::new()
            .route("/graphql", post(handlers::graphql_handler))
            .route("/crud/:db_alias/:col/create",
                   post(handlers::crud_create_handler))
            .route("/crud/:db_alias/:col/read",
                   post(handlers::crud_read_handler))
            .route("/crud/:db_alias/:col/update",
                   post(handlers::crud_update_handler))
            .route("/crud/:db_alias/:col/delete",
                   post(handlers::crud_delete_handler))
            .route("/eventing/queue",
                   post(handlers::eventing_queue_handler))
            .route("/files/upload",
                   post(handlers::filestore_upload_handler))
            .route("/files/download/*path",
                   get(handlers::filestore_download_handler));

        // Admin routes
        let admin_routes = Router::new()
            .route("/config/projects/:project/database/:db_alias",
                   post(handlers::admin_set_db_config_handler))
            .route("/login",
                   post(handlers::admin_login_handler))
            .layer(middleware::from_fn(admin_auth_middleware));

        // Health check
        let health_route = Router::new()
            .route("/health-check", get(handlers::health_check_handler));

        // Combine routes
        Router::new()
            .nest("/v1/api/:project", api_routes)
            .nest("/v1/config", admin_routes)
            .merge(health_route)
            .layer(middleware_stack)
            .with_state(self.app_state.clone())
    }
}
```

### 8.2 HTTP Handlers

```rust
// crates/gateway/src/handlers/crud.rs

use axum::{
    extract::{State, Path, Json},
    http::HeaderMap,
};

pub async fn crud_read_handler(
    State(state): State<Arc<AppState>>,
    Path((project, db_alias, col)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(req): Json<ReadRequest>,
) -> Result<Json<ReadResponse>> {
    // Extract request context
    let ctx = extract_context(&headers)?;

    // Get token
    let token = extract_token(&headers)?;

    // Get project modules
    let modules = state.get_project_modules(&project).await?;

    // Authorize
    let (post_process, params) = modules.auth
        .is_read_authorized(&ctx, &project, modules.crud.get_db_type(&db_alias)?,
                           &col, &token, &req)
        .await?;

    // Execute query
    let mut response = modules.crud
        .read(&ctx, &db_alias, &col, req, params)
        .await?;

    // Post-process (field filtering, encryption)
    for value in &mut response.data {
        modules.auth.post_process(&ctx, post_process.clone(), value).await?;
    }

    Ok(Json(response))
}

fn extract_context(headers: &HeaderMap) -> Result<Context> {
    let request_id = headers.get("X-Request-ID")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Ok(Context {
        request_id,
        timeout: Some(Duration::from_secs(30)),
        claims: None,
        metadata: HashMap::new(),
    })
}

fn extract_token(headers: &HeaderMap) -> Result<String> {
    headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or(Error::Auth(AuthError::InvalidToken(
            "Missing Authorization header".into()
        )))
}
```

### 8.3 GraphQL Integration

```rust
// crates/gateway/src/handlers/graphql.rs

use async_graphql::{
    EmptyMutation, EmptySubscription, Schema, Context as GqlContext, Object,
    dataloader::DataLoader,
};

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn db_read(
        &self,
        ctx: &GqlContext<'_>,
        db_alias: String,
        col: String,
        find: Value,
        options: Option<ReadOptions>,
    ) -> Result<Vec<Value>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let request_ctx = ctx.data::<Context>()?;
        let project = ctx.data::<String>()?;

        let modules = state.get_project_modules(project).await?;

        let req = ReadRequest {
            find,
            options: options.unwrap_or_default(),
        };

        let response = modules.crud
            .read(request_ctx, &db_alias, &col, req, RequestParams::default())
            .await?;

        Ok(response.data)
    }
}

pub type ApiSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema(state: Arc<AppState>) -> ApiSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(state)
        .finish()
}

pub async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    req: async_graphql_axum::GraphQLRequest,
) -> async_graphql_axum::GraphQLResponse {
    let ctx = extract_context(&headers).unwrap_or_default();
    let schema = build_schema(state);

    schema
        .execute(req.into_inner().data(ctx).data(project))
        .await
        .into()
}
```

---

## 9. Authentication & Authorization

### 9.1 JWT Handling

```rust
// crates/modules/src/auth/jwt.rs

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub id: String,
    pub role: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
    pub exp: Option<u64>,
    pub iat: Option<u64>,
}

pub struct JwtHandler {
    encoding_key: EncodingKey,
    decoding_keys: Vec<DecodingKey>,
}

impl JwtHandler {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_keys: vec![DecodingKey::from_secret(secret.as_bytes())],
        }
    }

    pub fn with_multiple_secrets(secrets: &[String]) -> Self {
        let encoding_key = EncodingKey::from_secret(secrets[0].as_bytes());
        let decoding_keys = secrets.iter()
            .map(|s| DecodingKey::from_secret(s.as_bytes()))
            .collect();

        Self {
            encoding_key,
            decoding_keys,
        }
    }

    pub fn create_token(&self, claims: TokenClaims) -> Result<String> {
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| Error::Auth(AuthError::InvalidToken(e.to_string())))
    }

    pub fn parse_token(&self, token: &str) -> Result<TokenClaims> {
        // Try each decoding key (for key rotation)
        let mut last_err = None;

        for key in &self.decoding_keys {
            match decode::<TokenClaims>(token, key, &Validation::default()) {
                Ok(data) => return Ok(data.claims),
                Err(e) => last_err = Some(e),
            }
        }

        Err(Error::Auth(AuthError::InvalidToken(
            last_err.unwrap().to_string()
        )))
    }
}
```

### 9.2 Rule Evaluation

```rust
// crates/modules/src/auth/rules.rs

use crate::config::Rule;

pub struct RuleEvaluator {
    crud: Arc<dyn CrudForAuth>,
}

impl RuleEvaluator {
    pub async fn evaluate(
        &self,
        ctx: &Context,
        rule: &Rule,
        claims: &TokenClaims,
        args: &Value,
    ) -> Result<bool> {
        match rule {
            Rule::Allow => Ok(true),
            Rule::Deny => Ok(false),
            Rule::Authenticated => Ok(claims.id != ""),

            Rule::Match { match_type, f1, f2 } => {
                let v1 = self.resolve_value(f1, claims, args)?;
                let v2 = self.resolve_value(f2, claims, args)?;
                self.compare(&v1, &v2, match_type)
            }

            Rule::And { clauses } => {
                for clause in clauses {
                    if !self.evaluate(ctx, clause, claims, args).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Rule::Or { clauses } => {
                for clause in clauses {
                    if self.evaluate(ctx, clause, claims, args).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Rule::Query { db_alias, col, find } => {
                let find_resolved = self.template_value(find, claims, args)?;
                let req = ReadRequest {
                    find: find_resolved,
                    options: ReadOptions::default(),
                };

                let response = self.crud.read(ctx, db_alias, col, req,
                                             RequestParams::default()).await?;
                Ok(response.count > 0)
            }

            Rule::Webhook { url, template } => {
                self.evaluate_webhook(ctx, url, template, claims, args).await
            }
        }
    }

    fn resolve_value(
        &self,
        value: &Value,
        claims: &TokenClaims,
        args: &Value,
    ) -> Result<Value> {
        match value {
            Value::String(s) if s.starts_with("args.") => {
                let path = &s[5..];
                self.json_path(args, path)
            }
            Value::String(s) if s.starts_with("auth.") => {
                let path = &s[5..];
                self.json_path(&serde_json::to_value(claims)?, path)
            }
            _ => Ok(value.clone()),
        }
    }

    fn compare(&self, v1: &Value, v2: &Value, match_type: &MatchType)
        -> Result<bool> {
        use MatchType::*;
        Ok(match match_type {
            Equal => v1 == v2,
            NotEqual => v1 != v2,
            GreaterThan => {
                let n1 = v1.as_f64().ok_or(Error::Validation {
                    field: "f1".into(),
                    message: "Not a number".into(),
                })?;
                let n2 = v2.as_f64().ok_or(Error::Validation {
                    field: "f2".into(),
                    message: "Not a number".into(),
                })?;
                n1 > n2
            }
            // ... other comparisons
        })
    }
}
```

---

## 10. Event System

### 10.1 Intent-Stage-Complete Pattern

```rust
// crates/modules/src/eventing/processor.rs

pub struct EventProcessor {
    config: Arc<EventingConfig>,
    crud: Arc<dyn CrudForEventing>,
    webhook_client: reqwest::Client,
    pubsub: Arc<PubSubClient>,

    // Channels
    intent_tx: mpsc::Sender<EventIntent>,
    staged_tx: mpsc::Sender<StagedEvent>,
    complete_tx: broadcast::Sender<CompletedEvent>,
}

impl EventProcessor {
    // Intent creation hook (before operation)
    pub async fn create_intent(&self, ctx: &Context, req: &EventIntent)
        -> Result<String> {
        let event_id = ksuid::Ksuid::generate().to_string();

        let event_doc = json!({
            "id": event_id,
            "type": req.event_type,
            "status": "intent",
            "payload": req.payload,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        self.crud.internal_create(
            ctx,
            &self.config.db_alias,
            &ctx.project,
            &self.config.col,
            CreateRequest {
                op: CreateOp::One,
                doc: event_doc,
            },
        ).await?;

        Ok(event_id)
    }

    // Stage hook (after operation)
    pub async fn stage_event(&self, ctx: &Context, event_id: &str,
                            error: Option<&str>) -> Result<()> {
        let status = if error.is_some() { "failed" } else { "staged" };

        self.crud.internal_update(
            ctx,
            &self.config.db_alias,
            &ctx.project,
            &self.config.col,
            UpdateRequest {
                find: json!({"id": event_id}),
                update: json!({
                    "$set": {
                        "status": status,
                        "error": error,
                    }
                }),
                op: UpdateOp::Set,
            },
        ).await?;

        // Send to staged processor
        if status == "staged" {
            let _ = self.staged_tx.send(StagedEvent { id: event_id.to_string() }).await;
        }

        Ok(())
    }

    // Background processor for staged events
    async fn process_staged_loop(
        mut staged_rx: mpsc::Receiver<StagedEvent>,
        config: Arc<EventingConfig>,
        crud: Arc<dyn CrudForEventing>,
        webhook_client: reqwest::Client,
        pubsub: Arc<PubSubClient>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(5));

        loop {
            select! {
                Some(event) = staged_rx.recv() => {
                    if let Err(e) = Self::process_staged_event(
                        &config, &crud, &webhook_client, &pubsub, &event
                    ).await {
                        tracing::error!("Failed to process staged event: {}", e);
                    }
                }
                _ = ticker.tick() => {
                    // Poll for orphaned staged events
                    Self::poll_staged_events(&config, &crud, &webhook_client, &pubsub).await?;
                }
                _ = shutdown.recv() => {
                    tracing::info!("Shutting down staged processor");
                    break Ok(());
                }
            }
        }
    }

    async fn process_staged_event(
        config: &EventingConfig,
        crud: &Arc<dyn CrudForEventing>,
        webhook_client: &reqwest::Client,
        pubsub: &Arc<PubSubClient>,
        event: &StagedEvent,
    ) -> Result<()> {
        // Fetch event details
        let response = crud.read(
            &Context::new(),
            &config.db_alias,
            &config.col,
            ReadRequest {
                find: json!({"id": event.id}),
                options: ReadOptions::default(),
            },
            RequestParams::default(),
        ).await?;

        let event_doc = response.data.first()
            .ok_or(Error::NotFound {
                resource_type: "event".into(),
                id: event.id.clone(),
            })?;

        // Get matching triggers
        let triggers = Self::match_triggers(config, event_doc)?;

        for trigger in triggers {
            // Template transformation
            let payload = Self::template_payload(&trigger, event_doc)?;

            // Invoke webhook
            let result = Self::invoke_webhook(
                webhook_client,
                &trigger.url,
                &payload,
            ).await;

            match result {
                Ok(_) => {
                    // Mark complete
                    crud.internal_update(
                        &Context::new(),
                        &config.db_alias,
                        &config.project,
                        &config.col,
                        UpdateRequest {
                            find: json!({"id": event.id}),
                            update: json!({"$set": {"status": "complete"}}),
                            op: UpdateOp::Set,
                        },
                    ).await?;

                    // Publish to realtime subscribers
                    pubsub.publish(&trigger.event_type, event_doc).await?;
                }
                Err(e) => {
                    tracing::error!("Webhook failed: {}", e);
                    // Implement retry logic here
                }
            }
        }

        Ok(())
    }

    async fn invoke_webhook(
        client: &reqwest::Client,
        url: &str,
        payload: &Value,
    ) -> Result<()> {
        let response = client
            .post(url)
            .json(payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::Network(NetworkError::WebhookFailed {
                status: response.status().as_u16(),
                body: response.text().await?,
            }));
        }

        Ok(())
    }
}
```

---

## 11. State Management

### 11.1 Application State

```rust
// crates/gateway/src/state.rs

pub struct AppState {
    pub managers: Arc<Managers>,
    pub modules: Arc<RwLock<HashMap<String, ProjectModules>>>,
    pub config: Arc<ArcSwap<Config>>,
}

impl AppState {
    pub async fn get_project_modules(&self, project_id: &str)
        -> Result<ProjectModules> {
        let modules = self.modules.read().await;
        modules.get(project_id)
            .cloned()
            .ok_or_else(|| Error::NotFound {
                resource_type: "project".into(),
                id: project_id.into(),
            })
    }

    pub async fn reload_project(&self, project_id: &str) -> Result<()> {
        let config = self.config.load();
        let project_config = config.projects.get(project_id)
            .ok_or_else(|| Error::NotFound {
                resource_type: "project".into(),
                id: project_id.into(),
            })?;

        // Build new modules
        let modules = self.build_project_modules(project_config).await?;

        // Atomic swap
        let mut projects = self.modules.write().await;
        projects.insert(project_id.to_string(), modules);

        Ok(())
    }

    async fn build_project_modules(&self, config: &Project)
        -> Result<ProjectModules> {
        // Build CRUD module
        let crud = Arc::new(
            CrudModule::new(
                &config.database_configs,
                /* schema */ Arc::clone(&todo!()),
                /* auth */ Arc::clone(&todo!()),
            ).await?
        );

        // Build Auth module
        let auth = Arc::new(
            AuthModule::new(
                self.managers.cluster_id.clone(),
                self.managers.node_id.clone(),
                crud.clone(),
                self.managers.admin.clone(),
            )
        );

        // ... build other modules

        Ok(ProjectModules {
            crud,
            auth,
            // ... other modules
        })
    }
}

#[derive(Clone)]
pub struct ProjectModules {
    pub crud: Arc<CrudModule>,
    pub auth: Arc<AuthModule>,
    pub schema: Arc<SchemaModule>,
    pub eventing: Arc<EventingModule>,
    pub filestore: Arc<FileStoreModule>,
    pub functions: Arc<FunctionsModule>,
    pub realtime: Arc<RealtimeModule>,
    pub graphql: Arc<GraphQLModule>,
}
```

---

## 12. Type Safety Enhancements

### 12.1 Type-State Pattern

```rust
// State machine for request lifecycle

pub struct Unauthenticated;
pub struct Authenticated;
pub struct Authorized;

pub struct Request<State> {
    ctx: Context,
    data: Value,
    _state: PhantomData<State>,
}

impl Request<Unauthenticated> {
    pub fn new(ctx: Context, data: Value) -> Self {
        Self {
            ctx,
            data,
            _state: PhantomData,
        }
    }

    pub async fn authenticate(
        self,
        auth: &AuthModule,
        token: &str,
    ) -> Result<Request<Authenticated>> {
        let claims = auth.parse_token(&self.ctx, token).await?;
        Ok(Request {
            ctx: self.ctx.with_claims(claims),
            data: self.data,
            _state: PhantomData,
        })
    }
}

impl Request<Authenticated> {
    pub async fn authorize(
        self,
        auth: &AuthModule,
        rule: &Rule,
    ) -> Result<Request<Authorized>> {
        let allowed = auth.evaluate_rule(&self.ctx, rule).await?;
        if !allowed {
            return Err(Error::Unauthorized {
                reason: "Rule evaluation failed".into(),
            });
        }
        Ok(Request {
            ctx: self.ctx,
            data: self.data,
            _state: PhantomData,
        })
    }
}

impl Request<Authorized> {
    pub async fn execute(
        self,
        crud: &CrudModule,
        db_alias: &str,
        col: &str,
    ) -> Result<Response> {
        let req: ReadRequest = serde_json::from_value(self.data)?;
        let response = crud.read(&self.ctx, db_alias, col, req,
                                RequestParams::default()).await?;
        Ok(Response { data: response })
    }
}
```

### 12.2 Newtypes for Safety

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DbAlias(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectionName(String);

impl ProjectId {
    pub fn new(id: String) -> Result<Self> {
        if id.is_empty() {
            return Err(Error::Validation {
                field: "project_id".into(),
                message: "Cannot be empty".into(),
            });
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Prevents mixing up different string types
pub fn get_database_config(
    project: &ProjectId,  // Can't accidentally pass DbAlias
    db_alias: &DbAlias,   // Can't accidentally pass ProjectId
) -> Result<DatabaseConfig> {
    // ...
}
```

---

## 13. Performance Optimizations

### 13.1 Connection Pooling

```rust
// Already handled by sqlx and mongodb drivers
// Additional optimization: warm up pools on startup

impl CrudModule {
    pub async fn warmup(&self) -> Result<()> {
        let drivers = self.drivers.read().await;

        let mut tasks = Vec::new();
        for (alias, driver) in drivers.iter() {
            let driver = driver.clone();
            let alias = alias.clone();

            let task = tokio::spawn(async move {
                // Execute simple query to warm up connection
                match driver.raw_query(
                    &Context::new(),
                    "SELECT 1",
                    vec![],
                ).await {
                    Ok(_) => {
                        tracing::info!("Warmed up driver: {}", alias);
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!("Failed to warm up {}: {}", alias, e);
                        Err(e)
                    }
                }
            });

            tasks.push(task);
        }

        // Wait for all warmups
        for task in tasks {
            let _ = task.await?;
        }

        Ok(())
    }
}
```

### 13.2 Query Batching with DataLoader

```rust
// crates/modules/src/crud/dataloader.rs

use async_graphql::dataloader::*;

pub struct UserLoader {
    crud: Arc<CrudModule>,
    db_alias: String,
}

#[async_trait]
impl Loader<String> for UserLoader {
    type Value = Value;
    type Error = Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Value>, Error> {
        // Batch query for all keys at once
        let req = ReadRequest {
            find: json!({
                "id": {
                    "in": keys
                }
            }),
            options: ReadOptions::default(),
        };

        let response = self.crud.read(
            &Context::new(),
            &self.db_alias,
            "users",
            req,
            RequestParams::default(),
        ).await?;

        // Map results back to keys
        let mut map = HashMap::new();
        for user in response.data {
            if let Some(id) = user.get("id").and_then(|v| v.as_str()) {
                map.insert(id.to_string(), user);
            }
        }

        Ok(map)
    }
}
```

### 13.3 Caching Layer

```rust
// crates/modules/src/cache.rs

use redis::aio::ConnectionManager;

pub struct CacheModule {
    redis: ConnectionManager,
    ttl: Duration,
}

impl CacheModule {
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut conn = self.redis.clone();
        let value: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await?;

        match value {
            Some(v) => Ok(Some(serde_json::from_str(&v)?)),
            None => Ok(None),
        }
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let mut conn = self.redis.clone();
        let serialized = serde_json::to_string(value)?;

        redis::cmd("SETEX")
            .arg(key)
            .arg(self.ttl.as_secs())
            .arg(serialized)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }
}

// Cached CRUD wrapper
pub struct CachedCrudModule {
    crud: Arc<CrudModule>,
    cache: Arc<CacheModule>,
}

impl CachedCrudModule {
    pub async fn read(&self, ctx: &Context, db_alias: &str, col: &str,
                     req: ReadRequest, params: RequestParams)
        -> Result<ReadResponse> {
        // Generate cache key
        let cache_key = format!("{}:{}:{}", db_alias, col,
                               serde_json::to_string(&req)?);

        // Try cache first
        if let Some(cached) = self.cache.get(&cache_key).await? {
            return Ok(cached);
        }

        // Cache miss - query database
        let response = self.crud.read(ctx, db_alias, col, req, params).await?;

        // Store in cache
        let _ = self.cache.set(&cache_key, &response).await;

        Ok(response)
    }
}
```

---

## 14. Testing Strategy

### 14.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rule_evaluation_allow() {
        let evaluator = RuleEvaluator::new(/* ... */);
        let rule = Rule::Allow;
        let claims = TokenClaims {
            id: "user123".into(),
            role: Some("admin".into()),
            extra: HashMap::new(),
            exp: None,
            iat: None,
        };

        let result = evaluator.evaluate(
            &Context::new(),
            &rule,
            &claims,
            &json!({}),
        ).await;

        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_jwt_parsing() {
        let handler = JwtHandler::new("secret");
        let claims = TokenClaims {
            id: "user123".into(),
            role: Some("user".into()),
            extra: HashMap::new(),
            exp: None,
            iat: None,
        };

        let token = handler.create_token(claims.clone()).unwrap();
        let parsed = handler.parse_token(&token).unwrap();

        assert_eq!(parsed.id, claims.id);
    }
}
```

### 14.2 Integration Tests

```rust
// tests/crud_tests.rs

use space_cloud_gateway::*;
use testcontainers::*;

#[tokio::test]
async fn test_crud_create_read() {
    // Start Postgres container
    let docker = clients::Cli::default();
    let postgres = docker.run(images::postgres::Postgres::default());

    let port = postgres.get_host_port_ipv4(5432);
    let conn_str = format!("postgresql://postgres:postgres@localhost:{}/test", port);

    // Build CRUD module
    let config = DatabaseConfig {
        id: "test".into(),
        db_alias: "pg".into(),
        db_type: DbType::Postgres,
        enabled: true,
        conn: conn_str,
        name: "test".into(),
        driver_config: DriverConfig::default(),
    };

    let crud = CrudModule::new(
        &[(String::from("pg"), config)].into_iter().collect(),
        /* ... */
    ).await.unwrap();

    // Create record
    let create_req = CreateRequest {
        op: CreateOp::One,
        doc: json!({
            "name": "John Doe",
            "age": 30,
        }),
    };

    let count = crud.create(&Context::new(), "pg", "users", create_req,
                           RequestParams::default()).await.unwrap();
    assert_eq!(count, 1);

    // Read record
    let read_req = ReadRequest {
        find: json!({"name": "John Doe"}),
        options: ReadOptions::default(),
    };

    let response = crud.read(&Context::new(), "pg", "users", read_req,
                            RequestParams::default()).await.unwrap();
    assert_eq!(response.count, 1);
    assert_eq!(response.data[0]["age"], 30);
}
```

### 14.3 Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_query_builder_never_panics(
        col in "[a-z]{1,10}",
        find in any::<Value>(),
    ) {
        let driver = SqlDriver::new(/* ... */);
        let req = ReadRequest {
            find,
            options: ReadOptions::default(),
        };

        // Should never panic, even with arbitrary input
        let _ = driver.build_select_query(&col, &req);
    }
}
```

---

## 15. Migration Path

### 15.1 Phased Approach

**Phase 1: Foundation (Months 1-2)**
- Core types and traits
- Configuration system
- Error handling
- Single database driver (PostgreSQL)
- Basic CRUD operations

**Phase 2: Core Features (Months 3-4)**
- All database drivers (MySQL, SQL Server, MongoDB)
- Authentication & JWT
- Basic authorization rules
- REST API endpoints
- HTTP server with Axum

**Phase 3: Advanced Features (Months 5-6)**
- GraphQL support
- Schema validation
- Rule evaluation (nested queries, webhooks)
- File storage (local, S3, GCS)
- Eventing system (intent-stage-complete)

**Phase 4: Distributed Systems (Months 7-8)**
- Configuration synchronization
- Redis pub/sub
- Multi-node clustering
- Leader election
- Config store (Kubernetes integration)

**Phase 5: Production Ready (Months 9-10)**
- Real-time subscriptions (WebSockets)
- Complete GraphQL implementation
- Metrics and telemetry
- Performance optimization
- Comprehensive testing
- Documentation

**Phase 6: Feature Parity (Months 11-12)**
- Runner integration
- CLI tool
- Let's Encrypt support
- Mission Control UI integration
- Migration tooling

### 15.2 Compatibility Strategy

**Option A: Big Bang Migration**
- Implement full feature parity
- Switch all traffic at once
- Requires extensive testing

**Option B: Gradual Migration**
- Deploy Rust version alongside Go
- Use feature flags to route traffic
- Migrate project-by-project
- Rollback capability

**Recommended: Option B with Proxy Pattern**

```rust
// Gateway proxy that routes to Go or Rust based on config
pub struct HybridGateway {
    rust_handler: Arc<RustHandler>,
    go_proxy: Arc<GoProxyClient>,
    migration_config: Arc<ArcSwap<MigrationConfig>>,
}

impl HybridGateway {
    pub async fn handle_request(&self, req: Request) -> Response {
        let config = self.migration_config.load();

        match config.should_route_to_rust(&req.project_id) {
            true => self.rust_handler.handle(req).await,
            false => self.go_proxy.forward(req).await,
        }
    }
}
```

### 15.3 Data Migration

**Configuration Migration:**
```rust
pub struct ConfigMigrator;

impl ConfigMigrator {
    pub fn migrate_from_go(go_config: &GoConfig) -> Result<Config> {
        // Parse Go YAML format
        // Transform to Rust types
        // Validate
        // Write to new format
        todo!()
    }
}
```

**Database Schema Migration:**
- Event tables remain compatible
- No schema changes required
- Wire protocol compatibility maintained

---

## Summary

This design document provides a comprehensive blueprint for rewriting UDE in idiomatic Rust. Key highlights:

1. **Trait-based architecture** replaces Go interfaces with flexible trait system
2. **Ownership and borrowing** eliminates data races without runtime overhead
3. **Async/await** provides structured concurrency with tokio runtime
4. **Type safety** leverages Rust's type system to prevent entire classes of bugs
5. **Performance** targets 2-3x throughput improvement through zero-copy and efficient abstractions
6. **Gradual migration** enables low-risk transition with rollback capability

The Rust implementation will maintain API compatibility with the Go version while delivering superior performance, safety, and maintainability.
