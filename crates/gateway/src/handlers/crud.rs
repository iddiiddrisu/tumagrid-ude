use crate::handlers::extract_context;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use ude_core::*;
use std::sync::Arc;

//═══════════════════════════════════════════════════════════
// CREATE HANDLER
//═══════════════════════════════════════════════════════════

pub async fn create_handler(
    State(state): State<Arc<AppState>>,
    Path((project, db_alias, collection)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> Result<Json<CreateResponse>> {
    let ctx = extract_context(&headers);

    tracing::info!(
        request_id = %ctx.request_id,
        project = %project,
        db_alias = %db_alias,
        collection = %collection,
        "Handling CREATE request"
    );

    // Get project modules
    let modules = state.get_project(&project)?;

    // TODO: Authorization check
    // let token = extract_token(&headers)?;
    // modules.auth.is_create_authorized(&ctx, &project, db_type, &collection, &token, &req).await?;

    // Execute create
    let count = modules
        .crud
        .create(&ctx, &db_alias, &collection, req, RequestParams::default())
        .await?;

    tracing::info!(
        request_id = %ctx.request_id,
        count = count,
        "CREATE completed successfully"
    );

    Ok(Json(CreateResponse { count }))
}

//═══════════════════════════════════════════════════════════
// READ HANDLER
//═══════════════════════════════════════════════════════════

pub async fn read_handler(
    State(state): State<Arc<AppState>>,
    Path((project, db_alias, collection)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(req): Json<ReadRequest>,
) -> Result<Json<ReadResponse>> {
    let ctx = extract_context(&headers);

    tracing::info!(
        request_id = %ctx.request_id,
        project = %project,
        db_alias = %db_alias,
        collection = %collection,
        "Handling READ request"
    );

    // Get project modules
    let modules = state.get_project(&project)?;

    // TODO: Authorization check
    // let token = extract_token(&headers)?;
    // let (post_process, params) = modules.auth
    //     .is_read_authorized(&ctx, &project, db_type, &collection, &token, &req)
    //     .await?;

    // Execute read
    let response = modules
        .crud
        .read(&ctx, &db_alias, &collection, req, RequestParams::default())
        .await?;

    // TODO: Post-process
    // for value in &mut response.data {
    //     modules.auth.post_process(&ctx, post_process.clone(), value).await?;
    // }

    tracing::info!(
        request_id = %ctx.request_id,
        count = response.count,
        "READ completed successfully"
    );

    Ok(Json(response))
}

//═══════════════════════════════════════════════════════════
// UPDATE HANDLER
//═══════════════════════════════════════════════════════════

pub async fn update_handler(
    State(state): State<Arc<AppState>>,
    Path((project, db_alias, collection)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<UpdateResponse>> {
    let ctx = extract_context(&headers);

    tracing::info!(
        request_id = %ctx.request_id,
        project = %project,
        db_alias = %db_alias,
        collection = %collection,
        "Handling UPDATE request"
    );

    // Get project modules
    let modules = state.get_project(&project)?;

    // TODO: Authorization check
    // let token = extract_token(&headers)?;
    // modules.auth.is_update_authorized(&ctx, &project, db_type, &collection, &token, &req).await?;

    // Execute update
    let count = modules
        .crud
        .update(&ctx, &db_alias, &collection, req, RequestParams::default())
        .await?;

    tracing::info!(
        request_id = %ctx.request_id,
        count = count,
        "UPDATE completed successfully"
    );

    Ok(Json(UpdateResponse { count }))
}

//═══════════════════════════════════════════════════════════
// DELETE HANDLER
//═══════════════════════════════════════════════════════════

pub async fn delete_handler(
    State(state): State<Arc<AppState>>,
    Path((project, db_alias, collection)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<DeleteResponse>> {
    let ctx = extract_context(&headers);

    tracing::info!(
        request_id = %ctx.request_id,
        project = %project,
        db_alias = %db_alias,
        collection = %collection,
        "Handling DELETE request"
    );

    // Get project modules
    let modules = state.get_project(&project)?;

    // TODO: Authorization check
    // let token = extract_token(&headers)?;
    // modules.auth.is_delete_authorized(&ctx, &project, db_type, &collection, &token, &req).await?;

    // Execute delete
    let count = modules
        .crud
        .delete(&ctx, &db_alias, &collection, req, RequestParams::default())
        .await?;

    tracing::info!(
        request_id = %ctx.request_id,
        count = count,
        "DELETE completed successfully"
    );

    Ok(Json(DeleteResponse { count }))
}

//═══════════════════════════════════════════════════════════
// RESPONSE TYPES
//═══════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateResponse {
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateResponse {
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub count: u64,
}
