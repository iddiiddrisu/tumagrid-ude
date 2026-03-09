/*!
 * Integration Manager
 *
 * WHY THIS EXISTS:
 * ================
 * Handles external integration hooks and webhooks.
 * Based on SpaceCloud's IntegrationManager which provides:
 * - Pre/post operation hooks
 * - Webhook invocation
 * - External service integration
 * - Custom business logic injection points
 *
 * HOOK SYSTEM:
 * Allows external systems to hook into UDE operations:
 * - admin-login: Hook into admin login
 * - admin-token: Hook into token generation
 * - config-auth: Hook into config operations
 * - crud-*: Hook into CRUD operations (create, read, update, delete)
 * - custom: Custom webhook invocations
 *
 * FUTURE:
 * - Kubernetes integration
 * - External auth providers
 * - Webhook management UI
 * - Custom integrations
 */

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use ude_core::*;

/// Hook configuration
///
/// WHY: Define which external services to call for which operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Hook {
    /// Hook ID
    pub id: String,

    /// Hook URL to call
    pub url: String,

    /// Hook type (webhook, function, etc.)
    #[serde(rename = "type")]
    pub hook_type: String,

    /// Resources this hook applies to
    pub resources: Vec<String>,

    /// Operations this hook applies to (create, read, update, delete, access)
    pub operations: Vec<String>,

    /// Whether hook is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Hook response
///
/// WHY: Standardized response from hook invocations.
#[derive(Debug, Clone)]
pub struct HookResponse {
    /// HTTP status code
    pub status: u16,

    /// Response body
    pub body: serde_json::Value,

    /// Whether hook was successfully invoked
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,
}

impl HookResponse {
    /// Check if response indicates success
    pub fn is_success(&self) -> bool {
        self.success && self.status >= 200 && self.status < 300
    }

    /// Get error if present
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
}

/// Integration Manager - External integration hooks
///
/// WHY: Extensibility point for custom business logic.
/// Allows external systems to hook into UDE operations.
pub struct IntegrationManager {
    // Registered hooks
    hooks: RwLock<HashMap<String, Hook>>,

    // HTTP client for webhook invocations
    http_client: reqwest::Client,
}

impl IntegrationManager {
    /// Create new integration manager
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(HashMap::new()),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    //═══════════════════════════════════════════════════════════
    // HOOK MANAGEMENT
    //═══════════════════════════════════════════════════════════

    /// Register a hook
    ///
    /// WHY: Allow external systems to register their webhook endpoints.
    pub fn register_hook(&self, hook: Hook) -> Result<()> {
        let mut hooks = self.hooks.write();

        tracing::info!(
            hook_id = %hook.id,
            url = %hook.url,
            "Registering integration hook"
        );

        hooks.insert(hook.id.clone(), hook);
        Ok(())
    }

    /// Unregister a hook
    pub fn unregister_hook(&self, hook_id: &str) -> Result<()> {
        let mut hooks = self.hooks.write();

        hooks.remove(hook_id).ok_or_else(|| Error::NotFound {
            resource_type: "hook".to_string(),
            id: hook_id.to_string(),
        })?;

        tracing::info!(hook_id = %hook_id, "Unregistered integration hook");

        Ok(())
    }

    /// List all hooks
    pub fn list_hooks(&self) -> Vec<Hook> {
        let hooks = self.hooks.read();
        hooks.values().cloned().collect()
    }

    /// Get a specific hook
    pub fn get_hook(&self, hook_id: &str) -> Option<Hook> {
        let hooks = self.hooks.read();
        hooks.get(hook_id).cloned()
    }

    //═══════════════════════════════════════════════════════════
    // HOOK INVOCATION
    //═══════════════════════════════════════════════════════════

    /// Invoke hooks for a resource and operation
    ///
    /// WHY: Call registered webhooks when operations occur.
    /// Allows external systems to inject custom logic.
    pub async fn invoke_hooks(
        &self,
        resource: &str,
        operation: &str,
        payload: serde_json::Value,
    ) -> Vec<HookResponse> {
        let hooks = self.hooks.read().clone();
        let mut responses = Vec::new();

        for hook in hooks.values() {
            if !hook.enabled {
                continue;
            }

            // Check if hook applies to this resource and operation
            if !hook.resources.contains(&resource.to_string())
                && !hook.resources.contains(&"*".to_string())
            {
                continue;
            }

            if !hook.operations.contains(&operation.to_string())
                && !hook.operations.contains(&"*".to_string())
            {
                continue;
            }

            // Invoke the hook
            let response = self.invoke_webhook(&hook.url, &payload).await;
            responses.push(response);
        }

        responses
    }

    /// Invoke a webhook URL
    ///
    /// WHY: Make HTTP POST request to webhook endpoint.
    async fn invoke_webhook(&self, url: &str, payload: &serde_json::Value) -> HookResponse {
        tracing::debug!(url = %url, "Invoking webhook");

        match self
            .http_client
            .post(url)
            .json(payload)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response
                    .json::<serde_json::Value>()
                    .await
                    .unwrap_or(serde_json::json!({}));

                tracing::debug!(url = %url, status = status, "Webhook invoked successfully");

                HookResponse {
                    status,
                    body,
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "Webhook invocation failed");

                HookResponse {
                    status: 500,
                    body: serde_json::json!({}),
                    success: false,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    //═══════════════════════════════════════════════════════════
    // SPECIFIC HOOK HANDLERS
    //═══════════════════════════════════════════════════════════

    /// Handle admin login hook
    ///
    /// WHY: Allow external auth providers to handle admin login.
    pub async fn handle_admin_login(
        &self,
        username: &str,
        password: &str,
    ) -> Option<HookResponse> {
        let payload = serde_json::json!({
            "username": username,
            "password": password,
        });

        let responses = self.invoke_hooks("admin-login", "access", payload).await;
        responses.into_iter().find(|r| r.is_success())
    }

    /// Handle config auth hook
    ///
    /// WHY: Allow external systems to authorize config operations.
    pub async fn handle_config_auth(
        &self,
        resource: &str,
        operation: &str,
        claims: serde_json::Value,
    ) -> Option<HookResponse> {
        let payload = serde_json::json!({
            "resource": resource,
            "operation": operation,
            "claims": claims,
        });

        let responses = self.invoke_hooks("config-auth", "access", payload).await;
        responses.into_iter().find(|r| r.is_success())
    }

    /// Handle CRUD operation hook
    ///
    /// WHY: Allow external systems to hook into CRUD operations.
    pub async fn handle_crud_operation(
        &self,
        operation: &str,
        db_alias: &str,
        collection: &str,
        data: serde_json::Value,
    ) -> Vec<HookResponse> {
        let payload = serde_json::json!({
            "operation": operation,
            "dbAlias": db_alias,
            "collection": collection,
            "data": data,
        });

        let resource = format!("crud-{}", operation);
        self.invoke_hooks(&resource, operation, payload).await
    }
}

impl Default for IntegrationManager {
    fn default() -> Self {
        Self::new()
    }
}
