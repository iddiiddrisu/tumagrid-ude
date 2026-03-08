use async_recursion::async_recursion;
use ude_core::*;

/// Rule evaluator for authorization
pub struct RuleEvaluator;

impl RuleEvaluator {
    #[async_recursion]
    pub async fn evaluate(
        ctx: &Context,
        rule: &Rule,
        claims: &TokenClaims,
        args: &serde_json::Value,
    ) -> Result<bool> {
        match rule {
            Rule::Allow => Ok(true),
            Rule::Deny => Ok(false),
            Rule::Authenticated => Ok(!claims.id.is_empty()),

            Rule::Match { match_type, f1, f2 } => {
                let v1 = Self::resolve_value(f1, claims, args)?;
                let v2 = Self::resolve_value(f2, claims, args)?;
                Self::compare(&v1, &v2, match_type)
            }

            Rule::And { clauses } => {
                for clause in clauses {
                    if !Self::evaluate(ctx, clause, claims, args).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Rule::Or { clauses } => {
                for clause in clauses {
                    if Self::evaluate(ctx, clause, claims, args).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Rule::Query { .. } => {
                // TODO: Implement nested query evaluation
                Err(Error::Internal(
                    "Nested query evaluation not yet implemented".to_string()
                ))
            }

            Rule::Webhook { .. } => {
                // TODO: Implement webhook evaluation
                Err(Error::Internal(
                    "Webhook evaluation not yet implemented".to_string()
                ))
            }
        }
    }

    fn resolve_value(
        value: &serde_json::Value,
        claims: &TokenClaims,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match value {
            Value::String(s) if s.starts_with("args.") => {
                let path = &s[5..];
                Self::json_path(args, path)
            }
            Value::String(s) if s.starts_with("auth.") => {
                let path = &s[5..];
                let claims_json = serde_json::to_value(claims)?;
                Self::json_path(&claims_json, path)
            }
            _ => Ok(value.clone()),
        }
    }

    fn json_path(value: &serde_json::Value, path: &str) -> Result<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            current = current.get(part).ok_or_else(|| Error::Validation {
                field: path.to_string(),
                message: format!("Path not found: {}", part),
            })?;
        }

        Ok(current.clone())
    }

    fn compare(
        v1: &serde_json::Value,
        v2: &serde_json::Value,
        match_type: &MatchType,
    ) -> Result<bool> {
        use serde_json::Value;

        Ok(match match_type {
            MatchType::Equal => v1 == v2,
            MatchType::NotEqual => v1 != v2,

            MatchType::GreaterThan => {
                let n1 = v1.as_f64().ok_or_else(|| Error::Validation {
                    field: "f1".to_string(),
                    message: "Not a number".to_string(),
                })?;
                let n2 = v2.as_f64().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not a number".to_string(),
                })?;
                n1 > n2
            }

            MatchType::GreaterThanOrEqual => {
                let n1 = v1.as_f64().ok_or_else(|| Error::Validation {
                    field: "f1".to_string(),
                    message: "Not a number".to_string(),
                })?;
                let n2 = v2.as_f64().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not a number".to_string(),
                })?;
                n1 >= n2
            }

            MatchType::LessThan => {
                let n1 = v1.as_f64().ok_or_else(|| Error::Validation {
                    field: "f1".to_string(),
                    message: "Not a number".to_string(),
                })?;
                let n2 = v2.as_f64().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not a number".to_string(),
                })?;
                n1 < n2
            }

            MatchType::LessThanOrEqual => {
                let n1 = v1.as_f64().ok_or_else(|| Error::Validation {
                    field: "f1".to_string(),
                    message: "Not a number".to_string(),
                })?;
                let n2 = v2.as_f64().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not a number".to_string(),
                })?;
                n1 <= n2
            }

            MatchType::In => {
                let arr = v2.as_array().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not an array".to_string(),
                })?;
                arr.contains(v1)
            }

            MatchType::NotIn => {
                let arr = v2.as_array().ok_or_else(|| Error::Validation {
                    field: "f2".to_string(),
                    message: "Not an array".to_string(),
                })?;
                !arr.contains(v1)
            }

            MatchType::Contains => {
                if let (Value::String(s), Value::String(pattern)) = (v1, v2) {
                    s.contains(pattern.as_str())
                } else if let (Value::Array(arr), _) = (v1, v2) {
                    arr.contains(v2)
                } else {
                    return Err(Error::Validation {
                        field: "values".to_string(),
                        message: "Contains requires string or array".to_string(),
                    });
                }
            }
        })
    }
}
