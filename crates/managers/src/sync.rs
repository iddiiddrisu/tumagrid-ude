/*!
 * Sync Manager
 *
 * WHY THIS EXISTS:
 * ================
 * Centralizes configuration management and coordinates all modules.
 * Based on SpaceCloud's SyncManager which handles:
 * - Config loading from store (file/kube)
 * - Hot-reloading with zero-downtime
 * - Module initialization and coordination
 * - Leader election (multi-node)
 *
 * RESPONSIBILITIES:
 * 1. Load config from file system or Kubernetes
 * 2. Initialize ProjectModules for each project
 * 3. Provide hot-reload capability via arc-swap
 * 4. Coordinate config changes across modules
 * 5. Handle cluster synchronization (future)
 */

use arc_swap::ArcSwap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use ude_core::*;
use ude_modules::{
    AuthModule, CrudModule, DataSourceRegistry, DatabaseExecutor, QueryExecutor, ServiceMesh,
    CacheExecutor,
};

use super::{AdminManager, IntegrationManager};

/// Project modules for a single project
///
/// WHY: Each project gets its own isolated module instances.
/// Matches what we had in AppState but now coordinated by SyncManager.
#[derive(Clone)]
pub struct ProjectModules {
    pub crud: Arc<CrudModule>,
    pub auth: Arc<AuthModule>,
    pub project_config: ProjectConfig,
    pub orchestration: Option<Arc<QueryExecutor>>,
    pub composite_queries: Arc<RwLock<HashMap<String, CompositeQuery>>>,
}

/// Sync Manager - Configuration orchestrator
///
/// WHY: Single source of truth for config and module coordination.
/// Instead of AppState doing initialization, SyncManager handles it.
pub struct SyncManager {
    // Cluster identification
    node_id: String,
    cluster_id: String,

    // Configuration with hot-reload support
    config: Arc<ArcSwap<Config>>,

    // Project modules (one per project)
    projects: Arc<RwLock<HashMap<String, ProjectModules>>>,

    // Service mesh (shared across all projects)
    mesh: Arc<ServiceMesh>,

    // Manager dependencies
    #[allow(dead_code)]
    admin: Arc<AdminManager>,
    #[allow(dead_code)]
    integration: Arc<IntegrationManager>,
}

impl SyncManager {
    /// Create new sync manager
    ///
    /// WHY: Initialize with config and start coordinating modules.
    /// This replaces scattered initialization in AppState.
    pub async fn new(
        node_id: String,
        cluster_id: String,
        config: Config,
        admin: Arc<AdminManager>,
        integration: Arc<IntegrationManager>,
    ) -> Result<Self> {
        // Create service mesh
        let mesh = Arc::new(ServiceMesh::new());

        // Start mesh background tasks
        let _mesh_handles = mesh.start_background_tasks();
        tracing::info!("Service mesh initialized by SyncManager");

        let manager = Self {
            node_id,
            cluster_id,
            config: Arc::new(ArcSwap::from_pointee(config.clone())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            mesh,
            admin,
            integration,
        };

        // Initialize all projects from config
        manager.initialize_projects(&config).await?;

        Ok(manager)
    }

    /// Get current config (lock-free read)
    ///
    /// WHY: arc-swap allows reading config without locks.
    /// This is critical for hot-reloading without blocking requests.
    pub fn get_config(&self) -> Arc<Config> {
        self.config.load_full()
    }

    /// Update config with hot-reload
    ///
    /// WHY: Allow config changes without restart.
    /// Atomically swap config and reinitialize affected projects.
    pub async fn update_config(&self, new_config: Config) -> Result<()> {
        tracing::info!("Hot-reloading configuration");

        // Atomic swap
        self.config.store(Arc::new(new_config.clone()));

        // Reinitialize projects with new config
        self.initialize_projects(&new_config).await?;

        tracing::info!("Configuration hot-reloaded successfully");
        Ok(())
    }

    /// Get project modules
    ///
    /// WHY: Modules need to access project-specific instances.
    /// This is what handlers will call to get CRUD, Auth, etc.
    pub fn get_project(&self, project_id: &str) -> Result<ProjectModules> {
        let projects = self.projects.read();
        projects.get(project_id).cloned().ok_or_else(|| Error::NotFound {
            resource_type: "project".to_string(),
            id: project_id.to_string(),
        })
    }

    /// Get service mesh
    ///
    /// WHY: Service mesh is shared across all projects.
    pub fn get_mesh(&self) -> Arc<ServiceMesh> {
        self.mesh.clone()
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get cluster ID
    pub fn cluster_id(&self) -> &str {
        &self.cluster_id
    }

    //═══════════════════════════════════════════════════════════
    // PRIVATE: Module Initialization
    //═══════════════════════════════════════════════════════════

    /// Initialize all projects from config
    ///
    /// WHY: Load each project's config and create module instances.
    /// This is what AppState.initialize_projects() was doing.
    async fn initialize_projects(&self, config: &Config) -> Result<()> {
        let mut projects = self.projects.write();
        projects.clear();

        for (project_id, project_config) in &config.projects {
            tracing::info!(
                project_id = %project_id,
                "Initializing project modules"
            );

            let modules = self
                .build_project_modules(config, project_id, project_config)
                .await?;
            projects.insert(project_id.clone(), modules);
        }

        tracing::info!(
            num_projects = projects.len(),
            "All project modules initialized"
        );

        Ok(())
    }

    /// Build modules for a single project
    ///
    /// WHY: Each project gets isolated CRUD, Auth, Orchestration instances.
    /// Extracted from AppState for cleaner separation.
    async fn build_project_modules(
        &self,
        root_config: &Config,
        project_id: &str,
        config: &Project,
    ) -> Result<ProjectModules> {
        // Build CRUD module
        let crud = Arc::new(
            CrudModule::new(project_id.to_string(), &config.database_configs).await?,
        );

        // Build Auth module
        let auth = Arc::new(AuthModule::new(
            self.cluster_id.clone(),
            self.node_id.clone(),
            &config.auths,
        )?);

        // Build orchestration module (if databases configured)
        let orchestration = if !config.database_configs.is_empty() {
            self.build_orchestration_module(root_config, crud.clone()).await?
        } else {
            None
        };

        // Load composite queries
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

    /// Build orchestration module with all data source executors
    ///
    /// WHY: Orchestration needs registry with database, service mesh, cache executors.
    /// This was scattered in AppState, now centralized here.
    async fn build_orchestration_module(
        &self,
        root_config: &Config,
        crud: Arc<CrudModule>,
    ) -> Result<Option<Arc<QueryExecutor>>> {
        let mut registry = DataSourceRegistry::new();

        // Add database executor (wraps CRUD module)
        let db_executor = Arc::new(DatabaseExecutor::new(crud));
        registry = registry.with_database(db_executor);

        // Add service mesh executor
        registry = registry.with_service_mesh(self.mesh.executor());

        // Add cache executor if Redis configured
        if let Some(cache_config) = root_config.cache_config.as_ref() {
            if cache_config.enabled {
                tracing::info!(
                    redis_url = %cache_config.conn,
                    "Adding Redis cache executor to orchestration registry"
                );

                match CacheExecutor::with_redis(&cache_config.conn).await {
                    Ok(cache_executor) => {
                        registry = registry.with_cache(Arc::new(cache_executor));
                        tracing::info!("Cache executor initialized successfully");
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to initialize cache executor, continuing without cache"
                        );
                    }
                }
            }
        }

        Ok(Some(Arc::new(QueryExecutor::new(Arc::new(registry)))))
    }
}
