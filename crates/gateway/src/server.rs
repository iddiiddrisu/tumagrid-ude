use crate::{handlers, state::AppState};
use axum::{
    routing::{delete, get, post},
    Router,
};
use ude_core::{*, error::NetworkError};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

pub struct Server {
    state: Arc<AppState>,
    port: u16,
}

impl Server {
    pub async fn new(
        node_id: String,
        cluster_id: String,
        config: Config,
        port: u16,
    ) -> Result<Self> {
        let state = Arc::new(AppState::new(node_id, cluster_id, config).await?);

        Ok(Self { state, port })
    }

    pub async fn start(self) -> Result<()> {
        let app = self.build_router();

        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], self.port));

        tracing::info!(address = %addr, "Starting HTTP server");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Network(NetworkError::ServerError(e.to_string())))?;

        axum::serve(listener, app)
            .await
            .map_err(|e| Error::Network(NetworkError::ServerError(e.to_string())))?;

        Ok(())
    }

    fn build_router(&self) -> Router {
        // Middleware stack
        let middleware = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::new().include_headers(true))
                    .on_response(DefaultOnResponse::new().include_headers(true)),
            )
            .layer(CompressionLayer::new())
            .layer(
                CorsLayer::permissive() // TODO: Make this configurable
            );

        // API routes
        let api_routes = Router::new()
            // CRUD operations
            .route("/crud/:db_alias/:collection/create", post(handlers::crud::create_handler))
            .route("/crud/:db_alias/:collection/read", post(handlers::crud::read_handler))
            .route("/crud/:db_alias/:collection/update", post(handlers::crud::update_handler))
            .route("/crud/:db_alias/:collection/delete", post(handlers::crud::delete_handler))
            // Orchestration (The Killer Feature!)
            .route("/orchestration", get(handlers::orchestration::list_queries))
            .route("/orchestration/:query_id", get(handlers::orchestration::get_query_info))
            .route("/orchestration/:query_id", post(handlers::orchestration::execute_query))
            // Health check
            .route("/health", get(handlers::health::health_check));

        // Metrics endpoint (no auth, no project context)
        let metrics_routes = Router::new()
            .route("/metrics", get(handlers::metrics::metrics));

        // Service mesh management endpoints (platform-wide)
        let mesh_routes = Router::new()
            .route("/mesh/services", get(handlers::mesh::list_services))
            .route("/mesh/services", post(handlers::mesh::register_service))
            .route("/mesh/services/:service_id", get(handlers::mesh::get_service))
            .route("/mesh/services/:service_id", delete(handlers::mesh::unregister_service))
            .route("/mesh/services/:service_id/health", get(handlers::mesh::get_service_health))
            .route("/mesh/services/:service_id/latency", get(handlers::mesh::get_service_latency));

        // Combine all routes
        Router::new()
            .merge(metrics_routes)
            .merge(mesh_routes)
            .nest("/v1/api/:project", api_routes)
            .layer(middleware)
            .with_state(self.state.clone())
    }
}
