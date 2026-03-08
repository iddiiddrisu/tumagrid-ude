mod mongo;
mod query_builder;
mod sql;

use async_trait::async_trait;
use parking_lot::RwLock;
use ude_core::*;
use std::collections::HashMap;
use std::sync::Arc;

pub use mongo::MongoDriver;
pub use sql::SqlDriver;

//═══════════════════════════════════════════════════════════
// CRUD MODULE
//═══════════════════════════════════════════════════════════

pub struct CrudModule {
    drivers: Arc<RwLock<HashMap<String, Arc<dyn CrudOperations>>>>,
    _project_id: String,
}

impl CrudModule {
    pub async fn new(
        project_id: String,
        configs: &HashMap<String, DatabaseConfig>,
    ) -> Result<Self> {
        let mut drivers = HashMap::new();

        for (alias, config) in configs {
            if !config.enabled {
                tracing::info!(
                    project = %project_id,
                    db_alias = %alias,
                    "Skipping disabled database"
                );
                continue;
            }

            tracing::info!(
                project = %project_id,
                db_alias = %alias,
                db_type = ?config.db_type,
                "Initializing database driver"
            );

            let driver: Arc<dyn CrudOperations> = match config.db_type {
                DbType::Postgres | DbType::Mysql | DbType::Sqlserver => {
                    Arc::new(SqlDriver::new(config).await?)
                }
                DbType::Mongo => Arc::new(MongoDriver::new(config).await?),
                DbType::Embedded => {
                    // TODO: Implement Embedded driver
                    return Err(Error::Internal(
                        "Embedded driver not yet implemented".to_string(),
                    ));
                }
            };

            drivers.insert(alias.clone(), driver);
        }

        Ok(Self {
            drivers: Arc::new(RwLock::new(drivers)),
            _project_id: project_id,
        })
    }

    pub fn get_driver(&self, db_alias: &str) -> Result<Arc<dyn CrudOperations>> {
        let drivers = self.drivers.read();
        drivers
            .get(db_alias)
            .cloned()
            .ok_or_else(|| Error::NotFound {
                resource_type: "database".to_string(),
                id: db_alias.to_string(),
            })
    }

    pub async fn read(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        mut req: ReadRequest,
        params: RequestParams,
    ) -> Result<ReadResponse> {
        // Apply org_id filter for multi-tenant isolation
        if let Some(org_id) = params.auth.get("org_id") {
            // Add org_id to the find filter
            if let Some(find_obj) = req.find.as_object_mut() {
                find_obj.insert("org_id".to_string(), org_id.clone());
            }

            tracing::debug!(
                org_id = %org_id,
                collection = %col,
                "Applied org_id filter for read operation"
            );
        }

        let driver = self.get_driver(db_alias)?;
        driver.read(ctx, col, req).await
    }

    pub async fn create(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        mut req: CreateRequest,
        params: RequestParams,
    ) -> Result<u64> {
        // Inject org_id and user_id into document(s) for multi-tenant isolation
        let org_id = params.auth.get("org_id");
        let user_id = params.auth.get("user_id");

        // Handle both single object and array of objects
        if let Some(doc_obj) = req.doc.as_object_mut() {
            // Single document
            if let Some(org_id) = org_id {
                if !doc_obj.contains_key("org_id") {
                    doc_obj.insert("org_id".to_string(), org_id.clone());
                }
            }
            if let Some(user_id) = user_id {
                if !doc_obj.contains_key("created_by") {
                    doc_obj.insert("created_by".to_string(), user_id.clone());
                }
            }

            tracing::debug!(
                org_id = ?org_id,
                collection = %col,
                "Injected org_id into document"
            );
        } else if let Some(doc_array) = req.doc.as_array_mut() {
            // Array of documents
            let doc_count = doc_array.len();
            for doc in doc_array {
                if let Some(doc_obj) = doc.as_object_mut() {
                    if let Some(org_id) = org_id {
                        if !doc_obj.contains_key("org_id") {
                            doc_obj.insert("org_id".to_string(), org_id.clone());
                        }
                    }
                    if let Some(user_id) = user_id {
                        if !doc_obj.contains_key("created_by") {
                            doc_obj.insert("created_by".to_string(), user_id.clone());
                        }
                    }
                }
            }

            tracing::debug!(
                org_id = ?org_id,
                collection = %col,
                doc_count = doc_count,
                "Injected org_id into documents"
            );
        }

        let driver = self.get_driver(db_alias)?;
        driver.create(ctx, col, req).await
    }

    pub async fn update(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        mut req: UpdateRequest,
        params: RequestParams,
    ) -> Result<u64> {
        // Apply org_id filter for multi-tenant isolation
        if let Some(org_id) = params.auth.get("org_id") {
            // Add org_id to the find filter
            if let Some(find_obj) = req.find.as_object_mut() {
                find_obj.insert("org_id".to_string(), org_id.clone());
            }

            tracing::debug!(
                org_id = %org_id,
                collection = %col,
                "Applied org_id filter for update operation"
            );
        }

        // Inject updated_by for audit trails
        if let Some(user_id) = params.auth.get("user_id") {
            if let Some(update_obj) = req.update.as_object_mut() {
                update_obj.insert("updated_by".to_string(), user_id.clone());
            }
        }

        let driver = self.get_driver(db_alias)?;
        driver.update(ctx, col, req).await
    }

    pub async fn delete(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        mut req: DeleteRequest,
        params: RequestParams,
    ) -> Result<u64> {
        // Apply org_id filter for multi-tenant isolation
        if let Some(org_id) = params.auth.get("org_id") {
            // Add org_id to the find filter
            if let Some(find_obj) = req.find.as_object_mut() {
                find_obj.insert("org_id".to_string(), org_id.clone());
            }

            tracing::debug!(
                org_id = %org_id,
                collection = %col,
                "Applied org_id filter for delete operation"
            );
        }

        let driver = self.get_driver(db_alias)?;
        driver.delete(ctx, col, req).await
    }

    pub async fn get_db_type(&self, db_alias: &str) -> Result<DbType> {
        let driver = self.get_driver(db_alias)?;
        Ok(driver.get_db_type())
    }
}

#[async_trait]
impl CrudForAuth for CrudModule {
    async fn read(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: ReadRequest,
        params: RequestParams,
    ) -> Result<ReadResponse> {
        self.read(ctx, db_alias, col, req, params).await
    }
}
