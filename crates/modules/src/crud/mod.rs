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
        req: ReadRequest,
        _params: RequestParams,
    ) -> Result<ReadResponse> {
        let driver = self.get_driver(db_alias)?;
        driver.read(ctx, col, req).await
    }

    pub async fn create(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: CreateRequest,
        _params: RequestParams,
    ) -> Result<u64> {
        let driver = self.get_driver(db_alias)?;
        driver.create(ctx, col, req).await
    }

    pub async fn update(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: UpdateRequest,
        _params: RequestParams,
    ) -> Result<u64> {
        let driver = self.get_driver(db_alias)?;
        driver.update(ctx, col, req).await
    }

    pub async fn delete(
        &self,
        ctx: &Context,
        db_alias: &str,
        col: &str,
        req: DeleteRequest,
        _params: RequestParams,
    ) -> Result<u64> {
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

#[async_trait]
impl CrudForEventing for CrudModule {
    async fn internal_create(
        &self,
        ctx: &Context,
        db_alias: &str,
        _project: &str,
        col: &str,
        req: CreateRequest,
    ) -> Result<()> {
        let driver = self.get_driver(db_alias)?;
        driver.create(ctx, col, req).await?;
        Ok(())
    }

    async fn internal_update(
        &self,
        ctx: &Context,
        db_alias: &str,
        _project: &str,
        col: &str,
        req: UpdateRequest,
    ) -> Result<()> {
        let driver = self.get_driver(db_alias)?;
        driver.update(ctx, col, req).await?;
        Ok(())
    }

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

    async fn get_db_type(&self, db_alias: &str) -> Result<DbType> {
        self.get_db_type(db_alias).await
    }
}
