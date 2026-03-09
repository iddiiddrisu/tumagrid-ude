use ude_core::*;
use ude_managers::{Managers, ProjectModules};
use std::sync::Arc;

//═══════════════════════════════════════════════════════════
// APPLICATION STATE
//═══════════════════════════════════════════════════════════

/// Application state - Thin wrapper around Managers
///
/// WHY: AppState now delegates to Managers instead of doing
/// initialization itself. Cleaner separation of concerns.
///
/// BEFORE: AppState had config loading, project initialization,
/// module coordination all mixed together.
///
/// AFTER: AppState just holds Managers and delegates.
/// SyncManager handles all the heavy lifting.
#[derive(Clone)]
pub struct AppState {
    managers: Arc<Managers>,
}

impl AppState {
    /// Create new app state
    ///
    /// WHY: Simplified to just create Managers and delegate everything.
    /// No more scattered initialization logic.
    pub async fn new(node_id: String, cluster_id: String, config: Config) -> Result<Self> {
        let managers = Arc::new(Managers::new(node_id, cluster_id, config).await?);

        Ok(Self { managers })
    }

    /// Get project modules
    ///
    /// WHY: Delegate to SyncManager instead of managing projects ourselves.
    pub fn get_project(&self, project_id: &str) -> Result<ProjectModules> {
        self.managers.sync.get_project(project_id)
    }

    /// Reload configuration with hot-reload
    ///
    /// WHY: Delegate to SyncManager for atomic config swapping.
    pub async fn reload_config(&self, new_config: Config) -> Result<()> {
        self.managers.sync.update_config(new_config).await
    }

    /// Get managers reference
    ///
    /// WHY: Some components may need direct access to managers.
    #[allow(dead_code)]
    pub fn managers(&self) -> &Managers {
        &self.managers
    }

    /// Get service mesh
    ///
    /// WHY: Mesh handlers need access to service registry.
    pub fn mesh(&self) -> Arc<ude_modules::ServiceMesh> {
        self.managers.sync.get_mesh()
    }

    /// Get config with lock-free read
    ///
    /// WHY: CORS and other components need config access.
    pub fn config(&self) -> Arc<Config> {
        self.managers.sync.get_config()
    }
}
