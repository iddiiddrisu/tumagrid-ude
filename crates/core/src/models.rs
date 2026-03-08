use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

//═══════════════════════════════════════════════════════════
// CONTEXT
//═══════════════════════════════════════════════════════════

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

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

//═══════════════════════════════════════════════════════════
// TOKEN CLAIMS
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
}

//═══════════════════════════════════════════════════════════
// DATABASE TYPES
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbType {
    Postgres,
    Mysql,
    Sqlserver,
    Mongo,
    Embedded,
}

//═══════════════════════════════════════════════════════════
// CRUD REQUEST/RESPONSE MODELS
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadRequest {
    pub find: serde_json::Value,
    #[serde(default)]
    pub options: ReadOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<serde_json::Value>,
    #[serde(default)]
    pub sort: Vec<String>,
    #[serde(default)]
    pub skip: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResponse {
    pub count: u64,
    pub data: Vec<serde_json::Value>,
    pub metadata: Option<SqlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlMetadata {
    pub affected_rows: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    pub op: CreateOp,
    pub doc: serde_json::Value,
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
    pub find: serde_json::Value,
    pub update: serde_json::Value,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub find: serde_json::Value,
    pub op: DeleteOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeleteOp {
    One,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateRequest {
    pub pipeline: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<BatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BatchOperation {
    Create {
        col: String,
        #[serde(flatten)]
        request: CreateRequest,
    },
    Update {
        col: String,
        #[serde(flatten)]
        request: UpdateRequest,
    },
    Delete {
        col: String,
        #[serde(flatten)]
        request: DeleteRequest,
    },
}

//═══════════════════════════════════════════════════════════
// REQUEST PARAMS
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default)]
pub struct RequestParams {
    pub auth: HashMap<String, serde_json::Value>,
    pub custom: HashMap<String, serde_json::Value>,
}

//═══════════════════════════════════════════════════════════
// POST PROCESS
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct PostProcess {
    pub actions: Vec<PostProcessAction>,
}

#[derive(Debug, Clone)]
pub struct PostProcessAction {
    pub action: String,
    pub field: String,
    pub value: Option<serde_json::Value>,
}

//═══════════════════════════════════════════════════════════
// SCHEMA TYPES
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDescription {
    pub fields: Vec<InspectorFieldType>,
    pub indices: Vec<IndexType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectorFieldType {
    pub name: String,
    pub field_type: String,
    pub is_nullable: bool,
    pub is_primary: bool,
    pub is_foreign_key: bool,
    pub is_unique: bool,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexType {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
}

pub type Fields = HashMap<String, Field>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub field_type: String,
    pub is_nullable: bool,
    pub is_primary: bool,
}

//═══════════════════════════════════════════════════════════
// EVENTING TYPES
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventIntent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StagedEvent {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct CompletedEvent {
    pub id: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEventRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub options: EventOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

//═══════════════════════════════════════════════════════════
// FILE STORE TYPES
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRequest {
    pub path: String,
    pub content: Vec<u8>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum FileOpType {
    Create,
    Read,
    Delete,
}

//═══════════════════════════════════════════════════════════
// NEWTYPES FOR TYPE SAFETY
//═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(id: String) -> crate::Result<Self> {
        if id.is_empty() {
            return Err(crate::Error::Validation {
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

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DbAlias(String);

impl DbAlias {
    pub fn new(alias: String) -> crate::Result<Self> {
        if alias.is_empty() {
            return Err(crate::Error::Validation {
                field: "db_alias".into(),
                message: "Cannot be empty".into(),
            });
        }
        Ok(Self(alias))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DbAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectionName(String);

impl CollectionName {
    pub fn new(name: String) -> crate::Result<Self> {
        if name.is_empty() {
            return Err(crate::Error::Validation {
                field: "collection".into(),
                message: "Cannot be empty".into(),
            });
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CollectionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
