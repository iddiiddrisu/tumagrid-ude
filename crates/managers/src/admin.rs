/*!
 * Admin Manager
 *
 * WHY THIS EXISTS:
 * ================
 * Handles administrative operations and admin user authentication.
 * Based on SpaceCloud's AdminManager which manages:
 * - Admin user credentials
 * - Admin API authentication
 * - Service registry
 * - Environment information
 *
 * FUTURE:
 * - Admin console authentication
 * - Project management APIs
 * - User management
 * - Service discovery
 */

use parking_lot::RwLock;
use std::sync::Arc;

use super::SyncManager;

/// Admin Manager - Administrative operations
///
/// WHY: Separate admin concerns from business logic.
/// Admin operations have different auth, different lifecycle.
pub struct AdminManager {
    node_id: String,
    cluster_id: String,

    // Circular dependency with SyncManager (set after creation)
    sync_manager: RwLock<Option<Arc<SyncManager>>>,
}

impl AdminManager {
    /// Create new admin manager
    pub fn new(node_id: String, cluster_id: String) -> Self {
        Self {
            node_id,
            cluster_id,
            sync_manager: RwLock::new(None),
        }
    }

    /// Set sync manager reference
    ///
    /// WHY: Circular dependency between Admin and Sync managers.
    /// Admin needs Sync for config operations.
    pub fn set_sync_manager(&self, sync: Arc<SyncManager>) {
        let mut manager = self.sync_manager.write();
        *manager = Some(sync);
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get cluster ID
    pub fn cluster_id(&self) -> &str {
        &self.cluster_id
    }

    // TODO: Add admin operations
    // - authenticate_admin()
    // - create_project()
    // - delete_project()
    // - list_projects()
    // - get_cluster_status()
}
