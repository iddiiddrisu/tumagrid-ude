/*!
 * Admin Manager
 *
 * WHY THIS EXISTS:
 * ================
 * Handles administrative operations and admin user authentication.
 * Based on SpaceCloud's AdminManager which manages:
 * - Admin user credentials and authentication
 * - Admin JWT token generation and validation
 * - Cluster-level operations
 * - Service registry
 * - Config validation
 *
 * RESPONSIBILITIES:
 * 1. Admin login/authentication
 * 2. JWT token management (create, parse, refresh)
 * 3. Authorization checks (is admin?)
 * 4. Cluster information
 * 5. Config validation
 */

use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ude_core::*;

use super::{IntegrationManager, SyncManager};

/// Claims for admin JWT tokens
///
/// WHY: Standard JWT structure with admin-specific claims.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdminClaims {
    /// User ID (admin username)
    pub id: String,

    /// Role (should be "admin")
    pub role: String,

    /// Expiration time (Unix timestamp)
    pub exp: u64,

    /// Issued at (Unix timestamp)
    pub iat: u64,
}

/// Login response
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
}

/// Admin Manager - Administrative operations
///
/// WHY: Separate admin concerns from business logic.
/// Admin operations have different auth, different lifecycle.
pub struct AdminManager {
    node_id: String,
    cluster_id: String,

    // Admin configuration
    admin_config: RwLock<AdminConfig>,

    // Circular dependency with SyncManager (set after creation)
    sync_manager: RwLock<Option<Arc<SyncManager>>>,

    // Integration manager for hooks
    #[allow(dead_code)]
    integration_manager: RwLock<Option<Arc<IntegrationManager>>>,
}

impl AdminManager {
    /// Create new admin manager
    pub fn new(node_id: String, cluster_id: String) -> Self {
        Self {
            node_id,
            cluster_id,
            admin_config: RwLock::new(AdminConfig::default()),
            sync_manager: RwLock::new(None),
            integration_manager: RwLock::new(None),
        }
    }

    /// Set sync manager reference
    ///
    /// WHY: Circular dependency between Admin and Sync managers.
    /// Admin needs Sync for config operations.
    pub fn set_sync_manager(&self, sync: Arc<SyncManager>) {
        let mut manager = self.sync_manager.write();
        *manager = Some(sync.clone());

        // Update admin config from sync manager's config
        let config = sync.get_config();
        let mut admin_config = self.admin_config.write();
        *admin_config = config.cluster_config.admin.clone();
    }

    /// Set integration manager reference
    pub fn set_integration_manager(&self, integration: Arc<IntegrationManager>) {
        let mut manager = self.integration_manager.write();
        *manager = Some(integration);
    }

    //═══════════════════════════════════════════════════════════
    // AUTHENTICATION
    //═══════════════════════════════════════════════════════════

    /// Admin login
    ///
    /// WHY: Authenticate admin users and return JWT token.
    /// This is for admin console / CLI authentication.
    pub fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let admin_config = self.admin_config.read();

        // Check credentials
        if admin_config.user != username || admin_config.pass != password {
            return Err(Error::Auth(error::AuthError::InvalidCredentials));
        }

        // Create token
        let token = self.create_token(username, "admin", &admin_config.secret)?;

        tracing::info!(username = %username, "Admin login successful");

        Ok(LoginResponse { token })
    }

    /// Create JWT token with claims
    ///
    /// WHY: Generate JWT tokens for admin operations.
    fn create_token(&self, user_id: &str, role: &str, secret: &str) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = AdminClaims {
            id: user_id.to_string(),
            role: role.to_string(),
            exp: now + (7 * 24 * 60 * 60), // 7 days
            iat: now,
        };

        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| Error::Auth(error::AuthError::TokenGeneration(e.to_string())))
    }

    /// Parse and validate JWT token
    ///
    /// WHY: Validate admin tokens for protected endpoints.
    fn parse_token(&self, token: &str, secret: &str) -> Result<AdminClaims> {
        let validation = jsonwebtoken::Validation::default();

        let token_data = jsonwebtoken::decode::<AdminClaims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|e| Error::Auth(error::AuthError::InvalidToken(e.to_string())))?;

        Ok(token_data.claims)
    }

    /// Validate admin token
    ///
    /// WHY: Check if token is valid and belongs to an admin.
    /// Returns claims if valid.
    pub fn validate_admin_token(&self, token: &str) -> Result<AdminClaims> {
        let admin_config = self.admin_config.read();

        // Skip validation in development if not enforced
        if !admin_config.enforce_auth {
            tracing::debug!("Admin auth not enforced (development mode)");
            return Ok(AdminClaims {
                id: "dev-admin".to_string(),
                role: "admin".to_string(),
                exp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 3600,
                iat: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            });
        }

        let claims = self.parse_token(token, &admin_config.secret)?;

        // Check if role is admin
        if !claims.role.contains("admin") {
            return Err(Error::Auth(error::AuthError::Unauthorized(
                "Admin role required".to_string(),
            )));
        }

        tracing::debug!(user_id = %claims.id, "Admin token validated");

        Ok(claims)
    }

    /// Refresh admin token
    ///
    /// WHY: Allow admins to refresh their tokens without re-login.
    pub fn refresh_token(&self, token: &str) -> Result<LoginResponse> {
        let admin_config = self.admin_config.read();
        let claims = self.parse_token(token, &admin_config.secret)?;

        // Create new token with same claims
        let new_token = self.create_token(&claims.id, &claims.role, &admin_config.secret)?;

        Ok(LoginResponse { token: new_token })
    }

    /// Get internal access token
    ///
    /// WHY: For internal service-to-service communication.
    pub fn get_internal_access_token(&self) -> Result<String> {
        let admin_config = self.admin_config.read();
        self.create_token("internal", "service", &admin_config.secret)
    }

    //═══════════════════════════════════════════════════════════
    // CLUSTER INFORMATION
    //═══════════════════════════════════════════════════════════

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get cluster ID
    pub fn cluster_id(&self) -> &str {
        &self.cluster_id
    }

    /// Get admin secret
    ///
    /// WHY: Some operations need the secret (e.g., integrations).
    pub fn get_secret(&self) -> String {
        let admin_config = self.admin_config.read();
        admin_config.secret.clone()
    }

    /// Get admin credentials
    ///
    /// WHY: For displaying in admin console or CLI.
    pub fn get_credentials(&self) -> (String, String) {
        let admin_config = self.admin_config.read();
        (admin_config.user.clone(), admin_config.pass.clone())
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        let admin_config = self.admin_config.read();
        admin_config.enforce_auth
    }

    //═══════════════════════════════════════════════════════════
    // CONFIG VALIDATION
    //═══════════════════════════════════════════════════════════

    /// Validate project configuration
    ///
    /// WHY: Ensure project config is valid before accepting.
    pub fn validate_project_config(&self, _project_config: &ProjectConfig) -> Result<()> {
        // TODO: Add validation logic
        // - Check project ID format
        // - Validate namespace
        // - Check for required fields
        Ok(())
    }

    /// Validate database configuration
    ///
    /// WHY: Ensure database config is valid.
    pub fn validate_database_config(
        &self,
        _database_configs: &std::collections::HashMap<String, DatabaseConfig>,
    ) -> Result<()> {
        // TODO: Add validation logic
        // - Check connection strings
        // - Validate enabled databases
        // - Check for conflicts
        Ok(())
    }

    /// Check if caching can be enabled
    ///
    /// WHY: Some features require specific configurations.
    pub fn can_enable_caching(&self) -> Result<()> {
        // TODO: Add checks
        // - Redis connection available
        // - No conflicts with other features
        Ok(())
    }
}
