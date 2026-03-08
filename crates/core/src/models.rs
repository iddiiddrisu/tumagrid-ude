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
// TOKEN CLAIMS (Multi-Tenant)
//═══════════════════════════════════════════════════════════

/// JWT token claims with multi-tenant organization support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// User ID (unique across all organizations)
    pub id: String,

    /// User email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// User display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    // ===== Multi-Tenancy Fields =====

    /// Current active organization ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,

    /// Current active organization slug (human-readable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,

    /// Role in the current organization (owner, admin, developer, viewer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_role: Option<String>,

    /// All organizations the user belongs to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orgs: Vec<OrgMembership>,

    /// Computed permissions for the current organization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,

    // ===== Namespace Isolation =====

    /// Namespaces the user has access to (for project isolation)
    /// If empty, user has access to "default" namespace only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<String>,

    // ===== Legacy/Compatibility =====

    /// Legacy role field (for backwards compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,

    /// Extra custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,

    // ===== Standard JWT Fields =====

    /// Expiration time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,

    /// Issued at time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
}

impl TokenClaims {
    /// Check if user has a specific permission in current organization
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// Check if user has a specific role in current organization
    pub fn has_role(&self, role: &str) -> bool {
        self.org_role.as_ref().map(|r| r == role).unwrap_or(false)
    }

    /// Check if user has any of the specified roles
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        if let Some(user_role) = &self.org_role {
            roles.iter().any(|r| *r == user_role)
        } else {
            false
        }
    }

    /// Check if user is owner of current organization
    pub fn is_org_owner(&self) -> bool {
        self.has_role("owner")
    }

    /// Check if user is admin or owner of current organization
    pub fn is_org_admin(&self) -> bool {
        self.has_any_role(&["owner", "admin"])
    }

    /// Get the current organization ID (panics if not set)
    pub fn org_id(&self) -> &str {
        self.org_id.as_ref().expect("org_id not set in token")
    }

    /// Try to get the current organization ID
    pub fn try_org_id(&self) -> Option<&str> {
        self.org_id.as_deref()
    }

    /// Check if user has access to a specific namespace
    pub fn has_namespace_access(&self, namespace: &str) -> bool {
        // If namespaces list is empty, assume access to "default" only
        if self.namespaces.is_empty() {
            return namespace == "default";
        }

        // Otherwise, check if namespace is in the list
        self.namespaces.iter().any(|ns| ns == namespace)
    }

    /// Get accessible namespaces (returns ["default"] if empty)
    pub fn get_namespaces(&self) -> Vec<&str> {
        if self.namespaces.is_empty() {
            vec!["default"]
        } else {
            self.namespaces.iter().map(|s| s.as_str()).collect()
        }
    }
}

/// Organization membership information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgMembership {
    /// Organization ID
    pub org_id: String,

    /// Organization slug (human-readable identifier)
    pub org_slug: String,

    /// Organization name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,

    /// User's role in this organization
    pub role: String,

    /// Permissions in this organization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
}

//═══════════════════════════════════════════════════════════
// ORGANIZATION MODELS
//═══════════════════════════════════════════════════════════

/// Organization (tenant boundary)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
}

/// Request to create an organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to invite a user to an organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteUserRequest {
    pub email: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
}

/// Organization invitation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub id: String,
    pub organization_id: String,
    pub email: String,
    pub role: String,
    pub status: InvitationStatus,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Rejected,
    Expired,
}

/// Organization member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationMember {
    pub id: String,
    pub organization_id: String,
    pub user_id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub role: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub created_at: u64,
}

//═══════════════════════════════════════════════════════════
// RBAC MODELS
//═══════════════════════════════════════════════════════════

/// Built-in system roles
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SystemRole {
    /// Organization owner (full access)
    Owner,
    /// Administrator (manage org & users)
    Admin,
    /// Developer (deploy & manage projects)
    Developer,
    /// Viewer (read-only access)
    Viewer,
    /// Custom role (defined per organization)
    Custom(String),
}

impl SystemRole {
    pub fn as_str(&self) -> &str {
        match self {
            SystemRole::Owner => "owner",
            SystemRole::Admin => "admin",
            SystemRole::Developer => "developer",
            SystemRole::Viewer => "viewer",
            SystemRole::Custom(name) => name,
        }
    }

    /// Get default permissions for a system role
    pub fn default_permissions(&self) -> Vec<&'static str> {
        match self {
            SystemRole::Owner => vec![
                "org:*",
                "projects:*",
                "crud:*",
                "auth:*",
                "deploy:*",
                "members:*",
                "settings:*",
            ],
            SystemRole::Admin => vec![
                "projects:*",
                "crud:*",
                "deploy:staging",
                "members:invite",
                "members:view",
                "settings:view",
            ],
            SystemRole::Developer => vec![
                "projects:read",
                "projects:write",
                "crud:read",
                "crud:write",
                "deploy:staging",
            ],
            SystemRole::Viewer => vec![
                "projects:read",
                "crud:read",
            ],
            SystemRole::Custom(_) => vec![],
        }
    }
}

/// Custom role definition (per organization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRole {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
    pub created_at: u64,
}

/// Permission check result
#[derive(Debug, Clone)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl PermissionCheck {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
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
    /// Filter for upsert operations (required when op is Upsert)
    /// WHY: Upsert needs to know which document to update or insert
    #[serde(skip_serializing_if = "Option::is_none")]
    pub find: Option<serde_json::Value>,
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
