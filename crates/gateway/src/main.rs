mod server;
mod handlers;
mod raw_handlers;
mod state;
mod telemetry;

use clap::Parser;
use ude_core::*;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ude-gateway")]
#[command(about = "UDE Gateway - Universal Developer Engine Backend-as-a-Service", long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, env = "CONFIG_PATH")]
    config: Option<PathBuf>,

    /// Server port
    #[arg(short, long, env = "PORT", default_value = "4122")]
    port: u16,

    /// Node ID
    #[arg(long, env = "NODE_ID")]
    node_id: Option<String>,

    /// Cluster ID
    #[arg(long, env = "CLUSTER_ID")]
    cluster_id: Option<String>,

    /// Log level
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    log_level: String,

    /// Log format (json or text)
    #[arg(long, env = "LOG_FORMAT", default_value = "json")]
    log_format: String,

    /// Development mode
    #[arg(long, env = "DEV")]
    dev: bool,

    /// Admin username
    #[arg(long, env = "ADMIN_USER", default_value = "admin")]
    admin_user: String,

    /// Admin password
    #[arg(long, env = "ADMIN_PASS", default_value = "admin")]
    admin_pass: String,

    /// Admin secret
    #[arg(long, env = "ADMIN_SECRET", default_value = "secret")]
    admin_secret: String,

    /// OpenTelemetry OTLP endpoint (e.g., http://otel-collector:4317)
    #[arg(long, env = "OTEL_ENDPOINT")]
    otel_endpoint: Option<String>,

    /// Enable Prometheus metrics endpoint
    #[arg(long, env = "ENABLE_PROMETHEUS", default_value = "true")]
    enable_prometheus: bool,

    /// Prometheus metrics port
    #[arg(long, env = "PROMETHEUS_PORT", default_value = "9090")]
    prometheus_port: u16,

    /// Trace sampling rate (0.0 to 1.0)
    #[arg(long, env = "TRACE_SAMPLE_RATE", default_value = "1.0")]
    trace_sample_rate: f64,

    /// Service environment (e.g., production, staging, development)
    #[arg(long, env = "ENVIRONMENT", default_value = "development")]
    environment: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize telemetry (tracing, metrics, logs)
    let telemetry_config = telemetry::TelemetryConfig {
        service_name: "spaceforge-gateway".to_string(),
        service_version: env!("CARGO_PKG_VERSION").to_string(),
        environment: cli.environment.clone(),
        otlp_endpoint: cli.otel_endpoint.clone(),
        enable_prometheus: cli.enable_prometheus,
        prometheus_port: cli.prometheus_port,
        trace_sample_rate: cli.trace_sample_rate,
        log_level: cli.log_level.clone(),
    };

    telemetry::init_telemetry(telemetry_config)?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting SpaceForge Gateway"
    );

    // Load configuration
    let config = load_config(cli.config.as_deref()).await?;

    // Generate node ID if not provided
    let node_id = cli.node_id.unwrap_or_else(|| {
        let id = format!("auto-{}", uuid::Uuid::new_v4());
        tracing::info!(node_id = %id, "Generated node ID");
        id
    });

    let cluster_id = cli.cluster_id.ok_or_else(|| {
        anyhow::anyhow!("Cluster ID is required. Set via --cluster-id or CLUSTER_ID env var")
    })?;

    tracing::info!(
        node_id = %node_id,
        cluster_id = %cluster_id,
        port = cli.port,
        "Initializing gateway"
    );

    // Create server
    let server = server::Server::new(node_id, cluster_id, config, cli.port).await?;

    // Start server
    server.start().await?;

    // Gracefully shutdown telemetry
    telemetry::shutdown_telemetry().await;

    Ok(())
}

async fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<Config> {
    if let Some(config_path) = path {
        tracing::info!(path = ?config_path, "Loading configuration from file");

        let content = tokio::fs::read_to_string(config_path).await?;

        let config: Config = if config_path.extension().and_then(|s| s.to_str()) == Some("yaml")
            || config_path.extension().and_then(|s| s.to_str()) == Some("yml")
        {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };

        Ok(config)
    } else {
        tracing::info!("Using default configuration");
        Ok(Config {
            projects: std::collections::HashMap::new(),
            ssl: None,
            cluster_config: ClusterConfig::default(),
            integrations: std::collections::HashMap::new(),
            cache_config: None,
        })
    }
}
