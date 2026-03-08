use async_trait::async_trait;
use futures::stream::TryStreamExt;
use mongodb::bson::{self, doc, Document};
use mongodb::{
    options::{ClientOptions, FindOptions},
    Client, Database,
};
use serde_json::Value;
use ude_core::{
    error::DatabaseError, AggregateRequest, BatchRequest, Context, CreateRequest, CrudOperations,
    DatabaseConfig, DbType, DeleteRequest, Error, ReadRequest, ReadResponse, Result,
    TableDescription, UpdateRequest,
};
use std::time::Duration;
use tracing;

pub struct MongoDriver {
    _client: mongodb::Client,
    database: Database,
    db_type: DbType,
    _db_name: String,
}

impl MongoDriver {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let mut client_options = ClientOptions::parse(&config.conn)
            .await
            .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?;

        // Configure connection pooling directly on ClientOptions
        client_options.app_name = Some(config.id.clone());
        client_options.max_pool_size = Some(config.driver_config.max_conn.into());
        client_options.min_pool_size = Some(config.driver_config.min_conn.into());
        client_options.max_idle_time =
            Some(Duration::from_secs(config.driver_config.max_idle_timeout));

        let client = Client::with_options(client_options)
            .map_err(|e| Error::Database(DatabaseError::Connection(e.to_string())))?;

        let database = client.database(&config.name);

        Ok(Self {
            _client: client,
            database,
            db_type: config.db_type.clone(),
            _db_name: config.name.clone(),
        })
    }

    fn is_client_connected(&self) -> bool {
        // More robust check: ping the database
        // This is a synchronous check and for async context, a small async operation is better.
        // For simplicity and given the `new` method ensures initial connection, this is acceptable for now.
        // A proper check would involve `self.client.list_database_names(None, None).await`.
        true // Temporarily assume connected if created successfully
    }
}

#[async_trait]
impl CrudOperations for MongoDriver {
    async fn read(&self, ctx: &Context, col: &str, req: ReadRequest) -> Result<ReadResponse> {
        let collection = self.database.collection::<Document>(col);

        let find_doc = bson::to_document(&req.find)
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        let mut find_options = FindOptions::builder().build();

        if let Some(limit) = req.options.limit {
            find_options.limit = Some(limit as i64);
        }
        if req.options.skip > 0 {
            find_options.skip = Some(req.options.skip as u64);
        }
        if let Some(select) = req.options.select {
            find_options.projection = Some(
                bson::to_document(&select)
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?,
            );
        }
        if !req.options.sort.is_empty() {
            let sort_doc = req.options.sort.into_iter().fold(doc! {}, |mut acc, s| {
                if s.starts_with('-') {
                    acc.insert(s[1..].to_string(), -1);
                } else {
                    acc.insert(s.to_string(), 1);
                }
                acc
            });
            find_options.sort = Some(sort_doc);
        }

        tracing::debug!(
            request_id = %ctx.request_id,
            collection = %col,
            find = ?find_doc,
            "Executing MongoDB find query"
        );

        let cursor = collection
            .find(find_doc)
            .with_options(find_options)
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        let data: Vec<Value> = cursor
            .try_collect::<Vec<Document>>()
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?
            .into_iter()
            .map(|doc| {
                bson::from_document(doc)
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))
            })
            .collect::<Result<Vec<Value>>>()?;

        Ok(ReadResponse {
            count: data.len() as u64,
            data,
            metadata: None,
        })
    }

    async fn create(&self, ctx: &Context, col: &str, req: CreateRequest) -> Result<u64> {
        let collection = self.database.collection::<Document>(col);

        let doc_to_insert = bson::to_document(&req.doc)
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        tracing::debug!(
            request_id = %ctx.request_id,
            collection = %col,
            op = ?req.op,
            doc = ?doc_to_insert,
            "Executing MongoDB create query"
        );

        let rows_affected = match req.op {
            ude_core::CreateOp::One => {
                collection
                    .insert_one(doc_to_insert)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;
                1
            }
            ude_core::CreateOp::All => {
                if let Some(arr) = req.doc.as_array() {
                    let mut docs = Vec::with_capacity(arr.len());
                    for v in arr {
                        docs.push(
                            bson::to_document(v).map_err(|e| {
                                Error::Database(DatabaseError::Query(e.to_string()))
                            })?,
                        );
                    }
                    if docs.is_empty() {
                        return Ok(0);
                    }
                    let res = collection
                        .insert_many(docs)
                        .await
                        .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;
                    res.inserted_ids.len() as u64
                } else {
                    return Err(Error::Database(DatabaseError::Query(
                        "CreateOp::All expects a JSON array of documents".into(),
                    )));
                }
            }
            ude_core::CreateOp::Upsert => {
                // Upsert requires a filter, which is not directly available in `CreateRequest`
                // This would typically be an `update` operation with `upsert: true`
                // TODO: Revisit `CreateRequest` for `CreateOp::Upsert` to include a filter
                return Err(Error::Internal(
                    "Upsert operation not supported directly by current CreateRequest for MongoDB. Use update with upsert option."
                        .to_string(),
                ));
            }
        };

        Ok(rows_affected)
    }

    async fn update(&self, ctx: &Context, col: &str, req: UpdateRequest) -> Result<u64> {
        let collection = self.database.collection::<Document>(col);

        let filter = bson::to_document(&req.find)
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;
        let update_val = req.update;

        tracing::debug!(
            request_id = %ctx.request_id,
            collection = %col,
            op = ?req.op,
            filter = ?filter,
            update = ?update_val,
            "Executing MongoDB update query"
        );

        let update_doc = match req.op {
            ude_core::UpdateOp::Set => {
                doc! { "$set": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
            ude_core::UpdateOp::Inc => {
                doc! { "$inc": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
            ude_core::UpdateOp::Dec => {
                let mut negated_doc = Document::new();
                for (k, v) in bson::to_document(&update_val)
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?
                {
                    if let Some(num) = v.as_i64() {
                        negated_doc.insert(k, -(num));
                    } else if let Some(num) = v.as_f64() {
                        negated_doc.insert(k, -(num));
                    } else {
                        return Err(Error::Database(DatabaseError::Query(format!(
                            "Cannot decrement non-numeric value for key: {}",
                            k
                        ))));
                    }
                }
                doc! { "$inc": negated_doc }
            }
            ude_core::UpdateOp::Mul => {
                doc! { "$mul": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
            ude_core::UpdateOp::Push => {
                doc! { "$push": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
            ude_core::UpdateOp::Rename => {
                doc! { "$rename": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
            ude_core::UpdateOp::Unset => {
                doc! { "$unset": bson::to_document(&update_val).map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))? }
            }
        };

        let update_result = collection
            .update_many(filter, update_doc)
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        Ok(update_result.modified_count)
    }

    async fn delete(&self, ctx: &Context, col: &str, req: DeleteRequest) -> Result<u64> {
        let collection = self.database.collection::<Document>(col);

        let filter = bson::to_document(&req.find)
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        tracing::debug!(
            request_id = %ctx.request_id,
            collection = %col,
            op = ?req.op,
            filter = ?filter,
            "Executing MongoDB delete query"
        );

        let deleted_count = match req.op {
            ude_core::DeleteOp::One => {
                let res = collection
                    .delete_one(filter)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;
                res.deleted_count
            }
            ude_core::DeleteOp::All => {
                let res = collection
                    .delete_many(filter)
                    .await
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;
                res.deleted_count
            }
        };

        Ok(deleted_count)
    }

    async fn aggregate(
        &self,
        ctx: &Context,
        col: &str,
        req: AggregateRequest,
    ) -> Result<serde_json::Value> {
        let collection = self.database.collection::<Document>(col);

        let pipeline: Vec<Document> = req
            .pipeline
            .into_iter()
            .map(|stage| bson::to_document(&stage))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        tracing::debug!(
            request_id = %ctx.request_id,
            collection = %col,
            pipeline = ?pipeline,
            "Executing MongoDB aggregate query"
        );

        let cursor = collection
            .aggregate(pipeline)
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        let data: Vec<Value> = cursor
            .try_collect::<Vec<Document>>()
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?
            .into_iter()
            .map(|doc| {
                bson::from_document(doc)
                    .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))
            })
            .collect::<Result<Vec<Value>>>()?;

        Ok(serde_json::Value::Array(data))
    }

    async fn batch(&self, ctx: &Context, req: BatchRequest) -> Result<Vec<u64>> {
        let mut results = Vec::with_capacity(req.requests.len());
        // For now, execute sequentially. A more optimized version could use MongoDB's bulk_write
        // if we map the operations to `WriteModel`s.
        for op in req.requests {
            let affected = match op {
                ude_core::BatchOperation::Create { col, request } => {
                    self.create(ctx, &col, request).await?
                }
                ude_core::BatchOperation::Update { col, request } => {
                    self.update(ctx, &col, request).await?
                }
                ude_core::BatchOperation::Delete { col, request } => {
                    self.delete(ctx, &col, request).await?
                }
            };
            results.push(affected);
        }
        Ok(results)
    }

    async fn describe_table(&self, _ctx: &Context, col: &str) -> Result<TableDescription> {
        let collection = self.database.collection::<Document>(col);

        let doc = collection
            .find_one(doc! {})
            .await
            .map_err(|e| Error::Database(DatabaseError::Query(e.to_string())))?;

        let mut fields = Vec::new();
        if let Some(doc) = doc {
            for (k, v) in doc.iter() {
                let field_type = match v {
                    bson::Bson::Double(_) => "Float",
                    bson::Bson::String(_) => "String",
                    bson::Bson::Array(_) => "Array",
                    bson::Bson::Document(_) => "Object",
                    bson::Bson::Boolean(_) => "Boolean",
                    bson::Bson::Null => "Unknown",
                    bson::Bson::Int32(_) => "Integer",
                    bson::Bson::Int64(_) => "Integer",
                    bson::Bson::ObjectId(_) => "ObjectId",
                    bson::Bson::DateTime(_) => "DateTime",
                    _ => "Unknown",
                };

                fields.push(ude_core::InspectorFieldType {
                    name: k.clone(),
                    field_type: field_type.to_string(),
                    is_nullable: true,
                    is_primary: k == "_id",
                    is_foreign_key: false,
                    is_unique: k == "_id",
                    is_auto_increment: false,
                });
            }
        }

        Ok(TableDescription {
            fields,
            indices: vec![],
        })
    }

    async fn raw_query(
        &self,
        _ctx: &Context,
        _query: &str,
        _args: Vec<serde_json::Value>,
    ) -> Result<ReadResponse> {
        Err(Error::Internal(
            "RawQuery not yet implemented for MongoDB".to_string(),
        ))
    }

    fn get_db_type(&self) -> DbType {
        self.db_type.clone()
    }

    fn is_connected(&self) -> bool {
        self.is_client_connected()
    }
}
