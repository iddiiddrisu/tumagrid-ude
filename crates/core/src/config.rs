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
    pub eventing_config: Option<EventingConfig>,
    #[serde(default)]
    pub eventing_triggers: HashMap<String, EventingTrigger>,
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
    Allow,
    Deny,
    Authenticated,
    Match {
        #[serde(rename = "type")]
        match_type: MatchType,
        f1: serde_json::Value,
        f2: serde_json::Value,
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
        find: serde_json::Value,
    },
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
// EVENTING CONFIG
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventingConfig {
    pub enabled: bool,
    pub db_alias: String,
    #[serde(default = "default_event_col")]
    pub col: String,
}

fn default_event_col() -> String {
    "event_logs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventingTrigger {
    pub id: String,
    pub event_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Rule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default = "default_retries")]
    pub retries: u32,
    #[serde(default = "default_timeout")]
    pub timeout: u64, // seconds
}

fn default_retries() -> u32 {
    3
}

fn default_timeout() -> u64 {
    10
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
}

fn default_telemetry() -> bool {
    true
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            letsencrypt_email: String::new(),
            enable_telemetry: default_telemetry(),
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
