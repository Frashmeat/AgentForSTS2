//! AgentTheSpire Web shell: health, shared Feature catalog and static SPA delivery.

mod static_files;

use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::{ConfigStatus, Settings, SettingsStore};
use ats_features::{FeatureContract, built_in_feature_contracts};
use axum::http::{HeaderValue, Method, header, request::Parts};
use axum::{Extension, Json, Router, routing::get};
use clap::Parser;
use serde::Serialize;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Debug, Parser)]
#[command(name = "ats-web", version, about = "AgentTheSpire Web shell")]
struct Args {
    #[arg(long, env = "ATS_HOST")]
    host: Option<String>,
    #[arg(long, env = "ATS_PORT")]
    port: Option<u16>,
    #[arg(long)]
    config: Option<PathBuf>,
}

struct AppState {
    config_status: ConfigStatus,
    settings: Settings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthReport {
    status: &'static str,
    role: &'static str,
    config_loaded: bool,
    config_errors: Vec<String>,
    llm_configured: bool,
    image_generation_configured: bool,
    feature_count: usize,
    project_execution_available: bool,
}

async fn health_handler(Extension(state): Extension<Arc<AppState>>) -> Json<HealthReport> {
    Json(HealthReport {
        status: "ok",
        role: "web",
        config_loaded: state.config_status.loaded,
        config_errors: state.config_status.errors.clone(),
        llm_configured: !state.settings.llm.api_key.is_empty(),
        image_generation_configured: !state.settings.image_gen.api_key.is_empty(),
        feature_count: built_in_feature_contracts().len(),
        project_execution_available: false,
    })
}

async fn features_handler() -> Json<Vec<FeatureContract>> {
    Json(built_in_feature_contracts())
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
    let (settings, config_status) = SettingsStore::load(args.config.as_deref());
    for error in &config_status.errors {
        tracing::warn!(%error, "configuration rejected");
    }
    let host = args
        .host
        .unwrap_or_else(|| settings.runtime.web.host.clone());
    let port = args.port.unwrap_or(settings.runtime.web.port);
    let cors_origins = settings.runtime.web.cors_origins.clone();
    let allow_loopback = settings.runtime.web.allow_loopback_origins;
    let state = Arc::new(AppState {
        config_status,
        settings,
    });

    let api = Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/features", get(features_handler))
        .layer(Extension(state));
    let app = Router::new()
        .merge(api)
        .fallback(static_files::static_handler)
        .layer(build_cors_layer(&cors_origins, allow_loopback))
        .layer(CatchPanicLayer::new());

    let address = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(%address, "ats-web listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn build_cors_layer(cors_origins: &[String], allow_loopback: bool) -> CorsLayer {
    let allowed = cors_origins.to_vec();
    let origin = AllowOrigin::predicate(move |origin: &HeaderValue, _parts: &Parts| {
        let Ok(value) = origin.to_str() else {
            return false;
        };
        allowed.iter().any(|allowed| allowed == value)
            || (allow_loopback && is_loopback_origin(value))
    });
    CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE])
        .allow_origin(origin)
}

fn is_loopback_origin(origin: &str) -> bool {
    let after_scheme = origin.split("://").nth(1).unwrap_or("");
    let host = if let Some(value) = after_scheme.strip_prefix('[') {
        value.split(']').next().unwrap_or("")
    } else {
        after_scheme.split(['/', ':']).next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl+C handler");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl+C"),
        () = terminate => tracing::info!("received SIGTERM"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_exposes_the_shared_catalog_without_project_execution_claims() {
        let report = HealthReport {
            status: "ok",
            role: "web",
            config_loaded: true,
            config_errors: Vec::new(),
            llm_configured: false,
            image_generation_configured: false,
            feature_count: built_in_feature_contracts().len(),
            project_execution_available: false,
        };
        assert_eq!(report.feature_count, 9);
        assert!(!report.project_execution_available);
    }

    #[test]
    fn loopback_origin_detection_is_exact() {
        assert!(is_loopback_origin("http://127.0.0.1:5173"));
        assert!(is_loopback_origin("http://[::1]:8080"));
        assert!(!is_loopback_origin("http://127.0.0.1.evil.example"));
    }
}
