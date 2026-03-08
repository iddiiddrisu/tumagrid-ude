mod jwt;
mod rules;

use async_trait::async_trait;
use parking_lot::RwLock;
use ude_core::{error::ConfigError, *};
use std::collections::HashMap;
use std::sync::Arc;

pub use jwt::JwtHandler;
pub use rules::RuleEvaluator;

//═══════════════════════════════════════════════════════════
// AUTH MODULE
//═══════════════════════════════════════════════════════════

pub struct AuthModule {
    _cluster_id: Arc<str>,
    node_id: Arc<str>,
    jwt_handler: JwtHandler,
    db_rules: Arc<RwLock<HashMap<String, DatabaseRule>>>,
}

impl AuthModule {
    pub fn new(
        cluster_id: String,
        node_id: String,
        auth_configs: &HashMap<String, AuthConfig>,
    ) -> Result<Self> {
        // Get primary auth config or use default
        let auth_config = auth_configs.values().next().ok_or_else(|| {
            Error::Config(ConfigError::MissingField(
                "No auth configuration found".to_string(),
            ))
        })?;

        let secrets = if auth_config.secrets.is_empty() {
            vec![auth_config.secret.clone()]
        } else {
            auth_config.secrets.clone()
        };

        Ok(Self {
            _cluster_id: Arc::from(cluster_id.as_str()),
            node_id: Arc::from(node_id.as_str()),
            jwt_handler: JwtHandler::new(&secrets),
            db_rules: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn set_database_rules(&self, rules: HashMap<String, DatabaseRule>) {
        let mut db_rules = self.db_rules.write();
        *db_rules = rules;
    }

    pub fn get_rule(&self, collection: &str) -> Option<DatabaseRule> {
        let rules = self.db_rules.read();
        rules.get(collection).cloned()
    }

    /// Build request params with auth context for multi-tenant data isolation
    fn build_request_params(&self, claims: &TokenClaims) -> RequestParams {
        let mut params = RequestParams::default();

        // Add user ID for user-level isolation
        params.auth.insert(
            "user_id".to_string(),
            serde_json::Value::String(claims.id.clone()),
        );

        // Add org_id for organization-level multi-tenancy
        if let Some(org_id) = &claims.org_id {
            params.auth.insert(
                "org_id".to_string(),
                serde_json::Value::String(org_id.clone()),
            );
        }

        // Add org_role for role-based filtering
        if let Some(org_role) = &claims.org_role {
            params.auth.insert(
                "org_role".to_string(),
                serde_json::Value::String(org_role.clone()),
            );
        }

        // Add permissions for fine-grained access control
        if !claims.permissions.is_empty() {
            let permissions: Vec<serde_json::Value> = claims
                .permissions
                .iter()
                .map(|p| serde_json::Value::String(p.clone()))
                .collect();
            params.auth.insert(
                "permissions".to_string(),
                serde_json::Value::Array(permissions),
            );
        }

        params
    }

    /// Create an internal token for UDE's internal operations
    pub fn create_internal_token(&self, _ctx: &Context) -> Result<String> {
        let claims = TokenClaims {
            id: "InternalUserID".to_string(),
            email: None,
            name: Some("UDE System".to_string()),
            org_id: None,
            org_slug: None,
            org_role: None,
            orgs: Vec::new(),
            permissions: Vec::new(),
            namespaces: Vec::new(), // Internal token has access to all namespaces
            role: Some("SpaceCloud".to_string()),
            extra: std::collections::HashMap::new(),
            exp: None,
            iat: Some(chrono::Utc::now().timestamp() as u64),
        };

        self.jwt_handler.create_token(claims)
    }

    /// Create a UDE access token
    pub fn create_sc_token(&self, _ctx: &Context) -> Result<String> {
        let claims = TokenClaims {
            id: self.node_id.to_string(),
            email: None,
            name: Some("UDE Node".to_string()),
            org_id: None,
            org_slug: None,
            org_role: None,
            orgs: Vec::new(),
            permissions: Vec::new(),
            namespaces: Vec::new(), // Node token has access to all namespaces
            role: Some("SpaceCloud".to_string()),
            extra: std::collections::HashMap::new(),
            exp: None,
            iat: Some(chrono::Utc::now().timestamp() as u64),
        };

        self.jwt_handler.create_token(claims)
    }
}

#[async_trait]
impl AuthOperations for AuthModule {
    async fn parse_token(&self, _ctx: &Context, token: &str) -> Result<TokenClaims> {
        self.jwt_handler.parse_token(token)
    }

    async fn create_token(&self, _ctx: &Context, claims: TokenClaims) -> Result<String> {
        self.jwt_handler.create_token(claims)
    }

    async fn is_read_authorized(
        &self,
        ctx: &Context,
        _project: &str,
        _db_type: DbType,
        col: &str,
        token: &str,
        _req: &ReadRequest,
    ) -> Result<(PostProcess, RequestParams)> {
        // Parse token
        let claims = self.parse_token(ctx, token).await?;

        // Get rule for collection
        let rule = self.get_rule(col).ok_or_else(|| Error::Unauthorized {
            reason: format!("No rule defined for collection: {}", col),
        })?;

        // Get read rule
        let read_rule = rule
            .rules
            .read
            .as_ref()
            .ok_or_else(|| Error::Unauthorized {
                reason: "No read rule defined".to_string(),
            })?;

        // TODO: Evaluate rule
        // For now, just allow if rule is "allow"
        match read_rule {
            Rule::Allow => {
                let post_process = PostProcess { actions: vec![] };
                let params = self.build_request_params(&claims);
                Ok((post_process, params))
            }
            Rule::Deny => Err(Error::Unauthorized {
                reason: "Access denied by rule".to_string(),
            }),
            Rule::Authenticated => {
                if claims.id.is_empty() {
                    Err(Error::Unauthorized {
                        reason: "Authentication required".to_string(),
                    })
                } else {
                    let params = self.build_request_params(&claims);
                    Ok((PostProcess { actions: vec![] }, params))
                }
            }
            _ => {
                // TODO: Implement full rule evaluation
                Err(Error::Internal(
                    "Complex rule evaluation not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn is_create_authorized(
        &self,
        ctx: &Context,
        _project: &str,
        _db_type: DbType,
        col: &str,
        token: &str,
        _req: &CreateRequest,
    ) -> Result<RequestParams> {
        let claims = self.parse_token(ctx, token).await?;

        let rule = self.get_rule(col).ok_or_else(|| Error::Unauthorized {
            reason: format!("No rule defined for collection: {}", col),
        })?;

        let create_rule = rule
            .rules
            .create
            .as_ref()
            .ok_or_else(|| Error::Unauthorized {
                reason: "No create rule defined".to_string(),
            })?;

        match create_rule {
            Rule::Allow => Ok(self.build_request_params(&claims)),
            Rule::Deny => Err(Error::Unauthorized {
                reason: "Access denied by rule".to_string(),
            }),
            Rule::Authenticated => {
                if claims.id.is_empty() {
                    Err(Error::Unauthorized {
                        reason: "Authentication required".to_string(),
                    })
                } else {
                    Ok(self.build_request_params(&claims))
                }
            }
            _ => Err(Error::Internal(
                "Complex rule evaluation not yet implemented".to_string(),
            )),
        }
    }

    async fn is_update_authorized(
        &self,
        ctx: &Context,
        _project: &str,
        _db_type: DbType,
        col: &str,
        token: &str,
        _req: &UpdateRequest,
    ) -> Result<RequestParams> {
        let claims = self.parse_token(ctx, token).await?;

        let rule = self.get_rule(col).ok_or_else(|| Error::Unauthorized {
            reason: format!("No rule defined for collection: {}", col),
        })?;

        let update_rule = rule
            .rules
            .update
            .as_ref()
            .ok_or_else(|| Error::Unauthorized {
                reason: "No update rule defined".to_string(),
            })?;

        match update_rule {
            Rule::Allow => Ok(self.build_request_params(&claims)),
            Rule::Deny => Err(Error::Unauthorized {
                reason: "Access denied by rule".to_string(),
            }),
            Rule::Authenticated => {
                if claims.id.is_empty() {
                    Err(Error::Unauthorized {
                        reason: "Authentication required".to_string(),
                    })
                } else {
                    Ok(self.build_request_params(&claims))
                }
            }
            _ => Err(Error::Internal(
                "Complex rule evaluation not yet implemented".to_string(),
            )),
        }
    }

    async fn is_delete_authorized(
        &self,
        ctx: &Context,
        _project: &str,
        _db_type: DbType,
        col: &str,
        token: &str,
        _req: &DeleteRequest,
    ) -> Result<RequestParams> {
        let claims = self.parse_token(ctx, token).await?;

        let rule = self.get_rule(col).ok_or_else(|| Error::Unauthorized {
            reason: format!("No rule defined for collection: {}", col),
        })?;

        let delete_rule = rule
            .rules
            .delete
            .as_ref()
            .ok_or_else(|| Error::Unauthorized {
                reason: "No delete rule defined".to_string(),
            })?;

        match delete_rule {
            Rule::Allow => Ok(self.build_request_params(&claims)),
            Rule::Deny => Err(Error::Unauthorized {
                reason: "Access denied by rule".to_string(),
            }),
            Rule::Authenticated => {
                if claims.id.is_empty() {
                    Err(Error::Unauthorized {
                        reason: "Authentication required".to_string(),
                    })
                } else {
                    Ok(self.build_request_params(&claims))
                }
            }
            _ => Err(Error::Internal(
                "Complex rule evaluation not yet implemented".to_string(),
            )),
        }
    }

    async fn post_process(
        &self,
        _ctx: &Context,
        _pp: PostProcess,
        _result: &mut serde_json::Value,
    ) -> Result<()> {
        // TODO: Implement post-processing (field filtering, encryption, etc.)
        Ok(())
    }
}
