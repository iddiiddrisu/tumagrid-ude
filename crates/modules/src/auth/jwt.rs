use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, Algorithm};
use ude_core::{*, error::AuthError};

pub struct JwtHandler {
    encoding_key: EncodingKey,
    decoding_keys: Vec<DecodingKey>,
}

impl JwtHandler {
    pub fn new(secrets: &[String]) -> Self {
        if secrets.is_empty() {
            panic!("At least one secret is required for JWT handler");
        }

        let encoding_key = EncodingKey::from_secret(secrets[0].as_bytes());
        let decoding_keys = secrets
            .iter()
            .map(|s| DecodingKey::from_secret(s.as_bytes()))
            .collect();

        Self {
            encoding_key,
            decoding_keys,
        }
    }

    pub fn create_token(&self, claims: TokenClaims) -> Result<String> {
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| Error::Auth(AuthError::InvalidToken(e.to_string())))
    }

    pub fn parse_token(&self, token: &str) -> Result<TokenClaims> {
        let mut last_err = None;

        // Try each decoding key (for key rotation)
        for key in &self.decoding_keys {
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_exp = false; // Don't validate expiry for now

            match decode::<TokenClaims>(token, key, &validation) {
                Ok(data) => {
                    // Check expiry manually if present
                    if let Some(exp) = data.claims.exp {
                        let now = chrono::Utc::now().timestamp() as u64;
                        if now > exp {
                            return Err(Error::Auth(AuthError::TokenExpired));
                        }
                    }
                    return Ok(data.claims);
                }
                Err(e) => last_err = Some(e),
            }
        }

        Err(Error::Auth(AuthError::InvalidToken(
            last_err.unwrap().to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_parse_token() {
        let handler = JwtHandler::new(&["secret".to_string()]);

        let claims = TokenClaims {
            id: "user123".to_string(),
            role: Some("admin".to_string()),
            extra: std::collections::HashMap::new(),
            exp: None,
            iat: Some(chrono::Utc::now().timestamp() as u64),
        };

        let token = handler.create_token(claims.clone()).unwrap();
        let parsed = handler.parse_token(&token).unwrap();

        assert_eq!(parsed.id, claims.id);
        assert_eq!(parsed.role, claims.role);
    }

    #[test]
    fn test_key_rotation() {
        let handler = JwtHandler::new(&["secret1".to_string(), "secret2".to_string()]);

        let claims = TokenClaims {
            id: "user123".to_string(),
            role: Some("user".to_string()),
            extra: std::collections::HashMap::new(),
            exp: None,
            iat: Some(chrono::Utc::now().timestamp() as u64),
        };

        // Create with first key
        let token = handler.create_token(claims.clone()).unwrap();

        // Should parse with either key
        let parsed = handler.parse_token(&token).unwrap();
        assert_eq!(parsed.id, claims.id);
    }

    #[test]
    fn test_expired_token() {
        let handler = JwtHandler::new(&["secret".to_string()]);

        let claims = TokenClaims {
            id: "user123".to_string(),
            role: Some("user".to_string()),
            extra: std::collections::HashMap::new(),
            exp: Some((chrono::Utc::now().timestamp() - 3600) as u64), // Expired 1 hour ago
            iat: Some(chrono::Utc::now().timestamp() as u64),
        };

        let token = handler.create_token(claims).unwrap();
        let result = handler.parse_token(&token);

        assert!(matches!(result, Err(Error::Auth(AuthError::TokenExpired))));
    }
}
