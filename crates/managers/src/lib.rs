/*!
 * UDE Managers
 *
 * WHY THIS EXISTS:
 * ================
 * Centralizes configuration management and module coordination.
 * Based on SpaceCloud's manager architecture which separates concerns:
 *
 * - SyncManager: Config loading, hot-reload, module coordination
 * - AdminManager: Admin operations, user auth
 * - IntegrationManager: External integration hooks
 *
 * PROBLEM IT SOLVES:
 * Without managers, config loading and module initialization is scattered
 * across AppState, main.rs, and individual modules. This creates:
 * - Unclear ownership of config
 * - Difficult hot-reloading
 * - Hard to test initialization
 * - No central coordination point
 */

mod admin;
mod integration;
mod sync;

// Re-export public types
pub use admin::{AdminClaims, AdminManager, LoginResponse};
pub use integration::{Hook, HookResponse, IntegrationManager};
pub use sync::{ProjectModules, SyncManager};

use std::sync::Arc;
use ude_core::*;

/// Managers holds all manager instances
///
/// WHY: Single point of access for all manager operations.
/// Follows SpaceCloud's pattern where main.rs creates Managers
/// and passes it to the HTTP server.
pub struct Managers {
    pub sync: Arc<SyncManager>,
    pub admin: Arc<AdminManager>,
    pub integration: Arc<IntegrationManager>,
}

impl Managers {
    /// Create new managers instance
    ///
    /// WHY: Initialize all managers with proper dependencies.
    /// SyncManager needs AdminManager and IntegrationManager,
    /// AdminManager needs SyncManager - circular dependency resolved
    /// by using Arc and setting references after creation.
    pub async fn new(
        node_id: String,
        cluster_id: String,
        config: Config,
    ) -> Result<Self> {
        // Create admin manager
        let admin = Arc::new(AdminManager::new(node_id.clone(), cluster_id.clone()));

        // Create integration manager
        let integration = Arc::new(IntegrationManager::new());

        // Create sync manager with config
        let sync = Arc::new(
            SyncManager::new(
                node_id,
                cluster_id,
                config,
                admin.clone(),
                integration.clone(),
            )
            .await?,
        );

        // Wire up circular dependencies
        admin.set_sync_manager(sync.clone());
        admin.set_integration_manager(integration.clone());

        Ok(Self {
            sync,
            admin,
            integration,
        })
    }
}
