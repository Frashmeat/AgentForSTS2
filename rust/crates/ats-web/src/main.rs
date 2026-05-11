//! ats-web — AgentTheSpire 的 Web 角色 HTTP 服务器。

mod static_files;

use std::path::PathBuf;
use std::sync::Arc;

use ats_core::config::{ConfigStatus, Settings};
use ats_core::health::{HealthReport, Role};
use axum::{Extension, Json, Router, routing::get};
use clap::Parser;
use tower_http::cors::CorsLayer;

#[derive(Debug, Parser)]
#[command(name = "ats-web", version, about = "AgentTheSpire Web server")]
struct Args {
    /// Bind host. Overrides config + env `ATS_HOST`.
    #[arg(long, env = "ATS_HOST")]
    host: Option<String>,

    /// Bind port. Overrides config + env `ATS_PORT`.
    #[arg(long, env = "ATS_PORT")]
    port: Option<u16>,

    /// Explicit path to JSON config file. Falls back to env `SPIREFORGE_CONFIG_PATH`
    /// or `./runtime/agentthespire.config.json`.
    #[arg(long)]
    config: Option<PathBuf>,
}

async fn health_handler(
    Extension(status): Extension<Arc<ConfigStatus>>,
) -> Json<HealthReport> {
    Json(ats_core::health::report(Role::Web, (*status).clone()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let (settings, mut config_status) = Settings::load(args.config.as_deref());
    let role_errors = settings.validate_for_role(Role::Web);
    if !role_errors.is_empty() {
        config_status.errors.extend(role_errors);
        config_status.loaded = false;
    }
    if config_status.errors.is_empty() {
        tracing::info!(
            "config loaded from {:?}",
            config_status.path.as_deref().unwrap_or("<unknown>")
        );
    } else {
        for err in &config_status.errors {
            tracing::warn!("config: {err}");
        }
    }
    let status_arc = Arc::new(config_status);

    let host = args
        .host
        .unwrap_or_else(|| settings.runtime.web.host.clone());
    let port = args.port.unwrap_or(settings.runtime.web.port);

    let api = Router::new()
        .route("/api/health", get(health_handler))
        .layer(Extension(Arc::clone(&status_arc)));

    let app = Router::new()
        .merge(api)
        .fallback(static_files::static_handler)
        .layer(CorsLayer::permissive());

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("ats-web listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
