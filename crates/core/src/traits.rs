use async_trait::async_trait;
use crate::{error::Result, models::*};

//═══════════════════════════════════════════════════════════
// CRUD TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait CrudOperations: Send + Sync {
    async fn create(&self, ctx: &Context, col: &str, req: CreateRequest) -> Result<u64>;

    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest) -> Result<ReadResponse>;

    async fn update(&self, ctx: &Context, col: &str, req: UpdateRequest) -> Result<u64>;

    async fn delete(&self, ctx: &Context, col: &str, req: DeleteRequest) -> Result<u64>;

    async fn aggregate(&self, ctx: &Context, col: &str, req: AggregateRequest) -> Result<serde_json::Value>;

    async fn batch(&self, ctx: &Context, req: BatchRequest) -> Result<Vec<u64>>;

    async fn describe_table(&self, ctx: &Context, col: &str) -> Result<TableDescription>;

    async fn raw_query(
        &self,
        ctx: &Context,
        query: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<ReadResponse>;

    fn get_db_type(&self) -> DbType;
    fn is_connected(&self) -> bool;
}

// Specialized traits for different module interactions
#[async_trait]
pub trait CrudForAuth: Send + Sync {
    async fn read(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: ReadRequest,
        params: RequestParams,
    ) -> Result<ReadResponse>;
}

#[async_trait]
pub trait CrudForEventing: Send + Sync {
    async fn internal_create(
        &self,
        ctx: &Context,
        db_alias: &str,
        project: &str,
        col: &str,
        req: CreateRequest,
    ) -> Result<()>;

    async fn internal_update(
        &self,
        ctx: &Context,
        db_alias: &str,
        project: &str,
        col: &str,
        req: UpdateRequest,
    ) -> Result<()>;

    async fn read(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: ReadRequest,
        params: RequestParams,
    ) -> Result<ReadResponse>;

    async fn get_db_type(&self, db_alias: &str) -> Result<DbType>;
}

#[async_trait]
pub trait CrudForSchema: Send + Sync {
    async fn get_db_type(&self, db_alias: &str) -> Result<DbType>;
    async fn raw_batch(&self, ctx: &Context, db_alias: &str, queries: Vec<String>) -> Result<()>;
    async fn describe_table(&self, ctx: &Context, db_alias: &str, col: &str) -> Result<TableDescription>;
}

//═══════════════════════════════════════════════════════════
// AUTH TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait AuthOperations: Send + Sync {
    async fn parse_token(&self, ctx: &Context, token: &str) -> Result<TokenClaims>;

    async fn create_token(&self, ctx: &Context, claims: TokenClaims) -> Result<String>;

    async fn is_read_authorized(
        &self,
        ctx: &Context,
        project: &str,
        db_type: DbType,
        col: &str,
        token: &str,
        req: &ReadRequest,
    ) -> Result<(PostProcess, RequestParams)>;

    async fn is_create_authorized(
        &self,
        ctx: &Context,
        project: &str,
        db_type: DbType,
        col: &str,
        token: &str,
        req: &CreateRequest,
    ) -> Result<RequestParams>;

    async fn is_update_authorized(
        &self,
        ctx: &Context,
        project: &str,
        db_type: DbType,
        col: &str,
        token: &str,
        req: &UpdateRequest,
    ) -> Result<RequestParams>;

    async fn is_delete_authorized(
        &self,
        ctx: &Context,
        project: &str,
        db_type: DbType,
        col: &str,
        token: &str,
        req: &DeleteRequest,
    ) -> Result<RequestParams>;

    async fn post_process(&self, ctx: &Context, pp: PostProcess, result: &mut serde_json::Value) -> Result<()>;
}

#[async_trait]
pub trait AuthForEventing: Send + Sync {
    async fn get_internal_token(&self, ctx: &Context) -> Result<String>;
    async fn is_eventing_authorized(
        &self,
        ctx: &Context,
        project: &str,
        token: &str,
        event: &QueueEventRequest,
    ) -> Result<RequestParams>;

    async fn create_token(&self, ctx: &Context, claims: TokenClaims) -> Result<String>;
}

#[async_trait]
pub trait AuthForCrud: Send + Sync {
    async fn post_process(&self, ctx: &Context, pp: PostProcess, result: &mut serde_json::Value) -> Result<()>;
}

//═══════════════════════════════════════════════════════════
// SCHEMA TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait SchemaOperations: Send + Sync {
    async fn validate_create(&self, ctx: &Context, db_type: DbType, col: &str, req: &CreateRequest) -> Result<()>;

    async fn validate_update(
        &self,
        ctx: &Context,
        db_type: DbType,
        col: &str,
        op: &str,
        update_doc: &serde_json::Value,
        find: &serde_json::Value,
    ) -> Result<()>;

    async fn post_process(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        result: &mut serde_json::Value,
    ) -> Result<()>;

    fn get_schema(&self, db_alias: &str, col: &str) -> Option<Fields>;
}

//═══════════════════════════════════════════════════════════
// EVENTING TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait EventingOperations: Send + Sync {
    async fn create_intent_hook(&self, ctx: &Context, req: &EventIntent) -> Result<String>;

    async fn hook_stage(&self, ctx: &Context, intent_id: &str, error: Option<&str>) -> Result<()>;

    async fn queue_event(&self, ctx: &Context, req: QueueEventRequest) -> Result<()>;
}

//═══════════════════════════════════════════════════════════
// FILESTORE TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait FileStoreOperations: Send + Sync {
    async fn upload(&self, ctx: &Context, req: UploadRequest) -> Result<()>;

    async fn download(&self, ctx: &Context, path: &str) -> Result<Vec<u8>>;

    async fn delete(&self, ctx: &Context, path: &str) -> Result<()>;

    async fn list(&self, ctx: &Context, prefix: &str) -> Result<Vec<FileInfo>>;
}

//═══════════════════════════════════════════════════════════
// SYNC MANAGER TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait SyncOperations: Send + Sync {
    async fn set_database_config(&self, project_id: &str, db_alias: &str, config: crate::DatabaseConfig) -> Result<()>;

    async fn get_assigned_ude_id(&self, ctx: &Context, project: &str, token: i32) -> Result<String>;

    fn get_node_id(&self) -> &str;
    fn is_leader(&self) -> bool;
}

//═══════════════════════════════════════════════════════════
// ADMIN TRAITS
//═══════════════════════════════════════════════════════════

#[async_trait]
pub trait AdminOperations: Send + Sync {
    async fn validate_admin_token(&self, token: &str) -> Result<()>;
    async fn is_leader_gateway(&self, node_id: &str) -> Result<bool>;
}
