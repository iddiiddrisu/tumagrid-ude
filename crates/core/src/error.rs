use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Infrastructure errors
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    // Domain errors
    #[error("Authentication failed: {0}")]
    Auth(#[from] AuthError),

    #[error("Authorization denied: {reason}")]
    Unauthorized { reason: String },

    #[error("Validation failed: {field}: {message}")]
    Validation { field: String, message: String },

    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound { resource_type: String, id: String },

    // Operational errors
    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Internal server error: {0}")]
    Internal(String),

    // Conversion errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Query failed: {0}")]
    Query(String),

    #[error("Transaction failed: {0}")]
    Transaction(String),

    #[error("Constraint violation: {0}")]
    Constraint(String),

    #[error("Database not found: {0}")]
    NotFound(String),

    #[error("Pool error: {0}")]
    Pool(String),
}

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("HTTP request failed: {0}")]
    Request(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Webhook failed with status {status}: {body}")]
    WebhookFailed { status: u16, body: String },

    #[error("Connection error: {0}")]
    Connection(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field {field}: {message}")]
    InvalidField { field: String, message: String },

    #[error("Load error: {0}")]
    LoadError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Missing required claim: {0}")]
    MissingClaim(String),

    #[error("Forbidden")]
    Forbidden,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Token generation failed: {0}")]
    TokenGeneration(String),
}

// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

// HTTP error responses
impl Error {
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Auth(_) => 401,
            Error::Unauthorized { .. } => 403,
            Error::NotFound { .. } => 404,
            Error::Validation { .. } => 400,
            Error::RateLimit => 429,
            Error::Timeout(_) => 504,
            _ => 500,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Error::Database(_) => "DB_ERROR",
            Error::Auth(_) => "AUTH_ERROR",
            Error::Unauthorized { .. } => "UNAUTHORIZED",
            Error::Validation { .. } => "VALIDATION_ERROR",
            Error::NotFound { .. } => "NOT_FOUND",
            Error::Timeout(_) => "TIMEOUT",
            Error::RateLimit => "RATE_LIMIT",
            Error::Network(_) => "NETWORK_ERROR",
            Error::Config(_) => "CONFIG_ERROR",
            _ => "INTERNAL_ERROR",
        }
    }
}

// Axum integration
impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.status_code())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        let body = serde_json::json!({
            "error": {
                "code": self.error_code(),
                "message": self.to_string(),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

// Helper conversion for common error types
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Internal(err.to_string())
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        Error::Timeout(std::time::Duration::from_secs(0))
    }
}
