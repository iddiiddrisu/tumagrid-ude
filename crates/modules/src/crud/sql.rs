use super::query_builder::SqlQueryBuilder;
use async_trait::async_trait;
use ude_core::{error::DatabaseError, *};
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions},
    postgres::{PgConnectOptions, PgPoolOptions},
    MySql, Pool, Postgres, Row,
};
use std::time::Duration;

//═══════════════════════════════════════════════════════════
// SQL DRIVER
//═══════════════════════════════════════════════════════════

pub enum SqlPool {
    Postgres(Pool<Postgres>),
    MySql(Pool<MySql>),
    // TODO: Add MSSQL support when SQLx adds it back
    // Mssql(Pool<Mssql>),
}

pub struct SqlDriver {
    pool: SqlPool,
    db_type: DbType,
    _db_name: String,
}

impl SqlDriver {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let pool = match config.db_type {
            DbType::Postgres => {
                let options = config
                    .conn
                    .parse::<PgConnectOptions>()
                    .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?
                    .database(&config.name);

                let pool = PgPoolOptions::new()
                    .max_connections(config.driver_config.max_conn)
                    .min_connections(config.driver_config.min_conn)
                    .idle_timeout(Duration::from_secs(config.driver_config.max_idle_timeout))
                    .acquire_timeout(Duration::from_secs(30))
                    .connect_with(options)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?;

                SqlPool::Postgres(pool)
            }
            DbType::Mysql => {
                let mut options = config
                    .conn
                    .parse::<MySqlConnectOptions>()
                    .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?;

                options = options.database(&config.name);

                let pool = MySqlPoolOptions::new()
                    .max_connections(config.driver_config.max_conn)
                    .min_connections(config.driver_config.min_conn)
                    .idle_timeout(Duration::from_secs(config.driver_config.max_idle_timeout))
                    .acquire_timeout(Duration::from_secs(30))
                    .connect_with(options)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?;

                SqlPool::MySql(pool)
            }
            DbType::Sqlserver => {
                // TODO: SQL Server support - waiting for SQLx 0.8+ feature
                return Err(Error::Database(DatabaseError::Connection(
                    "SQL Server support is not yet available in SQLx 0.8. Use Postgres or MySQL."
                        .to_string(),
                )));
            }
            _ => {
                return Err(Error::Database(DatabaseError::Connection(format!(
                    "Unsupported database type: {:?}",
                    config.db_type
                ))));
            }
        };

        Ok(Self {
            pool,
            db_type: config.db_type.clone(),
            _db_name: config.name.clone(),
        })
    }

    fn is_pool_closed(&self) -> bool {
        match &self.pool {
            SqlPool::Postgres(pool) => pool.is_closed(),
            SqlPool::MySql(pool) => pool.is_closed(),
            // SqlPool::Mssql(pool) => pool.is_closed(), // TODO: Re-enable when SQLx supports MSSQL
        }
    }
}

#[async_trait]
impl CrudOperations for SqlDriver {
    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest) -> Result<ReadResponse> {
        let builder = SqlQueryBuilder::new(&self.db_type);
        let (sql, bind_values) = builder.build_select(col, &req)?;

        tracing::debug!(
            request_id = %ctx.request_id,
            sql = %sql,
            table = %col,
            "Executing SELECT query"
        );

        let data = match &self.pool {
            SqlPool::Postgres(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_pg(query, value)?;
                }

                let rows = query
                    .fetch_all(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                rows.into_iter()
                    .map(|row| pg_row_to_json(&row))
                    .collect::<Result<Vec<_>>>()?
            }
            SqlPool::MySql(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_mysql(query, value)?;
                }

                let rows = query
                    .fetch_all(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                rows.into_iter()
                    .map(|row| mysql_row_to_json(&row))
                    .collect::<Result<Vec<_>>>()?
            }
        };

        Ok(ReadResponse {
            count: data.len() as u64,
            data,
            metadata: None,
        })
    }

    async fn create(&self, ctx: &Context, col: &str, req: CreateRequest) -> Result<u64> {
        let builder = SqlQueryBuilder::new(&self.db_type);
        let (sql, bind_values) = builder.build_insert(col, &req)?;

        tracing::debug!(
            request_id = %ctx.request_id,
            sql = %sql,
            table = %col,
            "Executing INSERT query"
        );

        let rows_affected = match &self.pool {
            SqlPool::Postgres(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_pg(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
            SqlPool::MySql(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_mysql(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
        };

        Ok(rows_affected)
    }

    async fn update(&self, ctx: &Context, col: &str, req: UpdateRequest) -> Result<u64> {
        let builder = SqlQueryBuilder::new(&self.db_type);
        let (sql, bind_values) = builder.build_update(col, &req)?;

        tracing::debug!(
            request_id = %ctx.request_id,
            sql = %sql,
            table = %col,
            "Executing UPDATE query"
        );

        let rows_affected = match &self.pool {
            SqlPool::Postgres(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_pg(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
            SqlPool::MySql(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_mysql(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
        };

        Ok(rows_affected)
    }

    async fn delete(&self, ctx: &Context, col: &str, req: DeleteRequest) -> Result<u64> {
        let builder = SqlQueryBuilder::new(&self.db_type);
        let (sql, bind_values) = builder.build_delete(col, &req)?;

        tracing::debug!(
            request_id = %ctx.request_id,
            sql = %sql,
            table = %col,
            "Executing DELETE query"
        );

        let rows_affected = match &self.pool {
            SqlPool::Postgres(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_pg(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
            SqlPool::MySql(pool) => {
                let mut query = sqlx::query(&sql);
                for value in &bind_values {
                    query = bind_json_value_mysql(query, value)?;
                }

                let result = query
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

                result.rows_affected()
            }
        };

        Ok(rows_affected)
    }

    async fn aggregate(
        &self,
        _ctx: &Context,
        _col: &str,
        _req: AggregateRequest,
    ) -> Result<serde_json::Value> {
        Err(Error::Internal(
            "Aggregate not yet implemented for SQL".to_string(),
        ))
    }

    async fn batch(&self, _ctx: &Context, _req: BatchRequest) -> Result<Vec<u64>> {
        Err(Error::Internal("Batch not yet implemented".to_string()))
    }

    async fn describe_table(&self, _ctx: &Context, _col: &str) -> Result<TableDescription> {
        Err(Error::Internal(
            "DescribeTable not yet implemented".to_string(),
        ))
    }

    async fn raw_query(
        &self,
        _ctx: &Context,
        _query: &str,
        _args: Vec<serde_json::Value>,
    ) -> Result<ReadResponse> {
        Err(Error::Internal("RawQuery not yet implemented".to_string()))
    }

    fn get_db_type(&self) -> DbType {
        self.db_type.clone()
    }

    fn is_connected(&self) -> bool {
        !self.is_pool_closed()
    }
}

//═══════════════════════════════════════════════════════════
// HELPER FUNCTIONS - BIND VALUES
//═══════════════════════════════════════════════════════════

fn bind_json_value_pg<'q>(
    mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    value: &'q serde_json::Value,
) -> Result<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>> {
    use serde_json::Value;

    match value {
        Value::Null => query = query.bind(None::<String>),
        Value::Bool(b) => query = query.bind(*b),
        Value::Number(n) if n.is_i64() => query = query.bind(n.as_i64().unwrap()),
        Value::Number(n) if n.is_f64() => query = query.bind(n.as_f64().unwrap()),
        Value::String(s) => query = query.bind(s.as_str()),
        _ => query = query.bind(value.to_string()),
    }

    Ok(query)
}

fn bind_json_value_mysql<'q>(
    mut query: sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>,
    value: &'q serde_json::Value,
) -> Result<sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>> {
    use serde_json::Value;

    match value {
        Value::Null => query = query.bind(None::<String>),
        Value::Bool(b) => query = query.bind(*b),
        Value::Number(n) if n.is_i64() => query = query.bind(n.as_i64().unwrap()),
        Value::Number(n) if n.is_f64() => query = query.bind(n.as_f64().unwrap()),
        Value::String(s) => query = query.bind(s.as_str()),
        _ => query = query.bind(value.to_string()),
    }

    Ok(query)
}

//═══════════════════════════════════════════════════════════
// HELPER FUNCTIONS - ROW TO JSON
//═══════════════════════════════════════════════════════════

fn pg_row_to_json(row: &sqlx::postgres::PgRow) -> Result<serde_json::Value> {
    use serde_json::{Map, Value};
    use sqlx::Column;

    let mut map = Map::new();

    for (i, col) in row.columns().iter().enumerate() {
        let col_name = col.name();

        // Try to get value as different types
        let value: Value = if let Ok(val) = row.try_get::<Option<String>, _>(i) {
            val.map(Value::String).unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<i64>, _>(i) {
            val.map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<f64>, _>(i) {
            val.and_then(|v| serde_json::Number::from_f64(v).map(Value::Number))
                .unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<bool>, _>(i) {
            val.map(Value::Bool).unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        map.insert(col_name.to_string(), value);
    }

    Ok(Value::Object(map))
}

fn mysql_row_to_json(row: &sqlx::mysql::MySqlRow) -> Result<serde_json::Value> {
    use serde_json::{Map, Value};
    use sqlx::Column;

    let mut map = Map::new();

    for (i, col) in row.columns().iter().enumerate() {
        let col_name = col.name();

        let value: Value = if let Ok(val) = row.try_get::<Option<String>, _>(i) {
            val.map(Value::String).unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<i64>, _>(i) {
            val.map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<f64>, _>(i) {
            val.and_then(|v| serde_json::Number::from_f64(v).map(Value::Number))
                .unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<Option<bool>, _>(i) {
            val.map(Value::Bool).unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        map.insert(col_name.to_string(), value);
    }

    Ok(Value::Object(map))
}

// fn mssql_row_to_json(row: &sqlx::mssql::MssqlRow) -> Result<serde_json::Value> {
//     use sqlx::Column;
//     use serde_json::{Map, Value};
//
//     let mut map = Map::new();
//
//     for (i, col) in row.columns().iter().enumerate() {
//         let col_name = col.name();
//
//         let value: Value = if let Ok(val) = row.try_get::<Option<String>, _>(i) {
//             val.map(Value::String).unwrap_or(Value::Null)
//         } else if let Ok(val) = row.try_get::<Option<i64>, _>(i) {
//             val.map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
//         } else if let Ok(val) = row.try_get::<Option<f64>, _>(i) {
//             val.and_then(|v| serde_json::Number::from_f64(v).map(Value::Number))
//                 .unwrap_or(Value::Null)
//         } else if let Ok(val) = row.try_get::<Option<bool>, _>(i) {
//             val.map(Value::Bool).unwrap_or(Value::Null)
//         } else {
//             Value::Null
//         };
//
//         map.insert(col_name.to_string(), value);
//     }
//
//     Ok(Value::Object(map))
// }
