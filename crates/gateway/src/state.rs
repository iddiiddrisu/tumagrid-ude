use arc_swap::ArcSwap;
use parking_lot::RwLock;
use ude_core::*;
use ude_modules::{
    AuthModule, CrudModule, DataSourceRegistry, DatabaseExecutor, QueryExecutor, ServiceMesh,
    CacheExecutor,
};
use std::collections::HashMap;
use std::sync::Arc;
use ude_core::CompositeQuery;

//═══════════════════════════════════════════════════════════
// APPLICATION STATE
//═══════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct AppState {
    pub node_id: Arc<str>,
    pub cluster_id: Arc<str>,
    #[allow(dead_code)]
    pub config: Arc<ArcSwap<Config>>,
    pub projects: Arc<RwLock<HashMap<String, ProjectModules>>>,
    pub mesh: Arc<ServiceMesh>,
}

impl AppState {
    pub async fn new(node_id: String, cluster_id: String, config: Config) -> Result<Self> {
        // Create service mesh
        let mesh = Arc::new(ServiceMesh::new());

        // Start mesh background tasks (health checking, latency tracking)
        let _mesh_handles = mesh.start_background_tasks();
        tracing::info!("Service mesh initialized and monitoring started");

        let state = Self {
            node_id: Arc::from(node_id.as_str()),
            cluster_id: Arc::from(cluster_id.as_str()),
            config: Arc::new(ArcSwap::from_pointee(config.clone())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            mesh,
        };

        // Initialize all projects
        state.initialize_projects(&config).await?;

        Ok(state)
    }

    async fn initialize_projects(&self, config: &Config) -> Result<()> {
        let mut projects = self.projects.write();

        for (project_id, project_config) in &config.projects {
            tracing::info!(project_id = %project_id, "Initializing project");

            let modules = self
                .build_project_modules(project_id, project_config)
                .await?;
            projects.insert(project_id.clone(), modules);

            tracing::info!(project_id = %project_id, "Project initialized successfully");
        }

        Ok(())
    }

    async fn build_project_modules(
        &self,
        project_id: &str,
        config: &Project,
    ) -> Result<ProjectModules> {
        // Build CRUD module
        let crud =
            Arc::new(CrudModule::new(project_id.to_string(), &config.database_configs).await?);

        // Build Auth module (simplified for now)
        let auth = Arc::new(AuthModule::new(
            self.cluster_id.to_string(),
            self.node_id.to_string(),
            &config.auths,
        )?);

        // Build Orchestration module
        let orchestration = self
            .build_orchestration_module(crud.clone(), config)
            .await?;

        // Load composite queries from config
        let composite_queries = Arc::new(RwLock::new(config.composite_queries.clone()));
        tracing::info!(
            num_queries = config.composite_queries.len(),
            "Loaded composite queries"
        );

        Ok(ProjectModules {
            crud,
            auth,
            project_config: config.project_config.clone(),
            orchestration,
            composite_queries,
        })
    }

    async fn build_orchestration_module(
        &self,
        crud: Arc<CrudModule>,
        _config: &Project,
    ) -> Result<Option<Arc<QueryExecutor>>> {
        // Build data source registry
        let mut registry = DataSourceRegistry::new();

        // Add database executor
        let db_executor = Arc::new(DatabaseExecutor::new(crud));
        registry = registry.with_database(db_executor);

        // Add service mesh executor (platform-wide resource)
        registry = registry.with_service_mesh(self.mesh.executor());

        // Add cache executor if Redis is configured
        let root_config = self.config.load();
        if let Some(cache_config) = root_config.cache_config.as_ref() {
            if cache_config.enabled {
                tracing::info!(redis_url = %cache_config.conn, "Initializing Redis cache executor");
                match CacheExecutor::with_redis(&cache_config.conn).await {
                    Ok(cache_executor) => {
                        registry = registry.with_cache(Arc::new(cache_executor));
                        tracing::info!("Cache executor initialized successfully");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to initialize cache executor, continuing without cache");
                    }
                }
            }
        }

        // Create query executor
        let executor = Arc::new(QueryExecutor::new(Arc::new(registry)));

        // For now, always return the executor
        // In the future, only return if composite queries are configured
        Ok(Some(executor))
    }

    pub fn get_project(&self, project_id: &str) -> Result<ProjectModules> {
        let projects = self.projects.read();
        projects
            .get(project_id)
            .cloned()
            .ok_or_else(|| Error::NotFound {
                resource_type: "project".to_string(),
                id: project_id.to_string(),
            })
    }

    #[allow(dead_code)]
    pub async fn reload_config(&self, new_config: Config) -> Result<()> {
        tracing::info!("Reloading configuration");

        // Initialize new projects
        self.initialize_projects(&new_config).await?;

        // Atomic swap
        self.config.store(Arc::new(new_config));

        tracing::info!("Configuration reloaded successfully");
        Ok(())
    }
}

//═══════════════════════════════════════════════════════════
// PROJECT MODULES
//═══════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct ProjectModules {
    pub crud: Arc<CrudModule>,
    pub auth: Arc<AuthModule>,
    pub project_config: ProjectConfig,
    pub orchestration: Option<Arc<QueryExecutor>>,
    pub composite_queries: Arc<RwLock<HashMap<String, CompositeQuery>>>,
}
