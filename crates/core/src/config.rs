use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::DbType;

//═══════════════════════════════════════════════════════════
// ROOT CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub projects: HashMap<String, Project>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl: Option<SslConfig>,
    pub cluster_config: ClusterConfig,
    #[serde(default)]
    pub integrations: HashMap<String, Integration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_config: Option<CacheConfig>,
}

//═══════════════════════════════════════════════════════════
// PROJECT CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub project_config: ProjectConfig,
    #[serde(default)]
    pub database_configs: HashMap<String, DatabaseConfig>,
    #[serde(default)]
    pub database_schemas: HashMap<String, DatabaseSchema>,
    #[serde(default)]
    pub database_rules: HashMap<String, DatabaseRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filestore_config: Option<FileStoreConfig>,
    #[serde(default)]
    pub filestore_rules: HashMap<String, FileRule>,
    #[serde(default)]
    pub auths: HashMap<String, AuthConfig>,
    #[serde(default)]
    pub ingress_routes: HashMap<String, IngressRoute>,
    #[serde(default)]
    pub remote_services: HashMap<String, RemoteService>,
    #[serde(default)]
    pub composite_queries: HashMap<String, crate::CompositeQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    /// Namespace for project isolation (defaults to "default")
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    #[serde(default)]
    pub aes_key: String,
    #[serde(default = "default_context_time")]
    pub context_time_graphql: u64, // milliseconds
}

fn default_namespace() -> String {
    "default".to_string()
}

fn default_context_time() -> u64 {
    30000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub name: String,
    pub value: String,
}

//═══════════════════════════════════════════════════════════
// DATABASE CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub id: String,
    pub db_alias: String,
    #[serde(rename = "type")]
    pub db_type: DbType,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub conn: String,
    pub name: String,
    #[serde(default)]
    pub driver_config: DriverConfig,
}

fn default_enabled() -> bool {
    true
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

fn default_max_conn() -> u32 {
    50
}

fn default_max_idle_timeout() -> u64 {
    300
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            max_conn: default_max_conn(),
            max_idle_timeout: default_max_idle_timeout(),
            min_conn: 0,
            max_idle_conn: 0,
        }
    }
}

//═══════════════════════════════════════════════════════════
// DATABASE SCHEMA
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub id: String,
    pub db_alias: String,
    pub col: String,
    pub schema: String, // JSON schema
}

//═══════════════════════════════════════════════════════════
// DATABASE RULES
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseRule {
    pub id: String,
    pub db_alias: String,
    pub col: String,
    pub rules: Rules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rules {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<Rule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<Rule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<Rule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "lowercase")]
pub enum Rule {
    // ===== Basic Rules =====
    Allow,
    Deny,
    Authenticated,

    // ===== Field Matching =====
    Match {
        #[serde(rename = "type")]
        match_type: MatchType,
        f1: serde_json::Value,
        f2: serde_json::Value,
    },

    // ===== Logical Operators =====
    And {
        clauses: Vec<Rule>,
    },
    Or {
        clauses: Vec<Rule>,
    },

    // ===== RBAC Rules (Multi-Tenant) =====

    /// Check if user has a specific permission in current organization
    #[serde(rename = "has_permission")]
    HasPermission {
        permission: String,
    },

    /// Check if user has a specific role in current organization
    #[serde(rename = "has_role")]
    HasRole {
        roles: Vec<String>,
    },

    /// Check if user is owner of current organization
    #[serde(rename = "org_owner")]
    OrgOwner,

    /// Check if user is admin (owner or admin role)
    #[serde(rename = "org_admin")]
    OrgAdmin,

    /// Check if resource belongs to user's organization
    #[serde(rename = "resource_owner")]
    ResourceOwner {
        /// Field in the resource containing organization_id
        field: String,
    },

    /// Check if user belongs to specific user (e.g., created_by, owner_id)
    #[serde(rename = "user_owner")]
    UserOwner {
        /// Field in the resource containing user_id
        field: String,
    },

    /// Allow cross-organization access for specific organizations
    #[serde(rename = "cross_org_access")]
    CrossOrgAccess {
        /// List of organization IDs that have access
        allowed_orgs: Vec<String>,
    },

    // ===== Advanced Rules (TODO) =====

    /// Query-based rule evaluation (not yet implemented)
    Query {
        db_alias: String,
        col: String,
        find: serde_json::Value,
    },

    /// Webhook-based rule evaluation (not yet implemented)
    Webhook {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
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

//═══════════════════════════════════════════════════════════
// FILE STORE CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStoreConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub store_type: FileStoreType,
    pub conn: String,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStoreType {
    Local,
    S3,
    Gcs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRule {
    pub id: String,
    pub prefix: String,
    pub rule: Rule,
}

//═══════════════════════════════════════════════════════════
// AUTH CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub id: String,
    pub secret: String,
    #[serde(default)]
    pub secrets: Vec<String>, // For key rotation
}

//═══════════════════════════════════════════════════════════
// INGRESS/ROUTING CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRoute {
    pub id: String,
    pub source: RouteSource,
    pub targets: Vec<RouteTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSource {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub host: String,
    pub port: u16,
    pub weight: u32,
}

//═══════════════════════════════════════════════════════════
// REMOTE SERVICES CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteService {
    pub id: String,
    pub url: String,
}

//═══════════════════════════════════════════════════════════
// CLUSTER CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    #[serde(default)]
    pub letsencrypt_email: String,
    #[serde(default = "default_telemetry")]
    pub enable_telemetry: bool,
    #[serde(default)]
    pub cors: CorsConfig,
}

fn default_telemetry() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
    #[serde(default = "default_max_age")]
    pub max_age: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: default_cors_enabled(),
            allowed_origins: vec![],
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age: default_max_age(),
        }
    }
}

fn default_cors_enabled() -> bool {
    true
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_allowed_headers() -> Vec<String> {
    vec![
        "Content-Type".to_string(),
        "Authorization".to_string(),
    ]
}

fn default_max_age() -> u64 {
    3600
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            letsencrypt_email: String::new(),
            enable_telemetry: default_telemetry(),
            cors: CorsConfig::default(),
        }
    }
}

//═══════════════════════════════════════════════════════════
// SSL CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

//═══════════════════════════════════════════════════════════
// CACHE CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub conn: String, // Redis connection string
    #[serde(default = "default_cache_ttl")]
    pub default_ttl: u64, // seconds
}

fn default_cache_ttl() -> u64 {
    300
}

//═══════════════════════════════════════════════════════════
// INTEGRATION CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integration {
    pub id: String,
    pub name: String,
}
