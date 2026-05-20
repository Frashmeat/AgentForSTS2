//! ats-web — AgentTheSpire 的 Web 角色 HTTP 服务器。

mod routes;
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

/// Shared per-process state attached via axum's `Extension` layer.
pub struct AppState {
    pub config_status: ConfigStatus,
    pub runtime_dir: PathBuf,
    /// 启动期加载的 Settings 快照，路由从此读 llm/auth 等字段。
    pub settings_snapshot: Option<Settings>,
}

async fn health_handler(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<HealthReport> {
    Json(build_health_report(&state))
}

/// 装 HealthReport：有 settings 时填全 readiness（LLM / image_gen），
/// active_project_open / image_proc_ready 在 Web 角色永为 false（前者是桌面端
/// 工程文件夹概念，后者是桌面端 ML prewarm）。
/// settings 缺失时回退到不带 readiness 的 report()，避免崩溃。
fn build_health_report(state: &AppState) -> HealthReport {
    match state.settings_snapshot.as_ref() {
        Some(settings) => ats_core::health::report_full(
            Role::Web,
            state.config_status.clone(),
            settings,
            false,
            false,
        ),
        None => ats_core::health::report(Role::Web, state.config_status.clone()),
    }
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

    let runtime_dir = config_status.runtime_dir();
    let host = args
        .host
        .unwrap_or_else(|| settings.runtime.web.host.clone());
    let port = args.port.unwrap_or(settings.runtime.web.port);
    let app_state = Arc::new(AppState {
        config_status: config_status.clone(),
        runtime_dir,
        settings_snapshot: Some(settings),
    });

    let api = Router::new()
        .route("/api/health", get(health_handler))
        .merge(routes::knowledge::router())
        .merge(routes::planning::router())
        .merge(routes::codegen::router())
        .merge(routes::llm::router())
        .layer(Extension(Arc::clone(&app_state)));

    let app = Router::new()
        .merge(api)
        .fallback(static_files::static_handler)
        .layer(CorsLayer::permissive());

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("ats-web listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("ats-web shut down cleanly");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_core::config::{ConfigStatus, Settings};

    fn make_state(settings: Option<Settings>) -> AppState {
        AppState {
            config_status: ConfigStatus {
                path: Some("/tmp/test.json".to_string()),
                file_present: true,
                loaded: settings.is_some(),
                errors: vec![],
            },
            runtime_dir: PathBuf::from("/tmp"),
            settings_snapshot: settings,
        }
    }

    #[test]
    fn build_health_report_with_configured_keys() {
        let mut settings = Settings::built_in_defaults();
        settings.llm.api_key = "sk-test-llm".to_string();
        settings.image_gen.api_key = "sk-test-img".to_string();

        let report = build_health_report(&make_state(Some(settings)));

        assert_eq!(report.role, Role::Web);
        assert!(report.readiness.llm_configured);
        assert!(report.readiness.image_gen_configured);
        assert!(!report.readiness.active_project_open, "Web 端永 false");
        assert!(!report.readiness.image_proc_ready, "Web 端无 ML prewarm");
        assert!(report.readiness.queue_worker_ready);
    }

    #[test]
    fn build_health_report_with_empty_keys() {
        let settings = Settings::built_in_defaults();
        let report = build_health_report(&make_state(Some(settings)));

        assert_eq!(report.role, Role::Web);
        assert!(!report.readiness.llm_configured);
        assert!(!report.readiness.image_gen_configured);
    }

    #[test]
    fn build_health_report_without_settings_snapshot_does_not_panic() {
        let report = build_health_report(&make_state(None));
        assert_eq!(report.role, Role::Web);
        // 无 settings 走 report() 分支，readiness 走 Default
        assert!(!report.readiness.llm_configured);
        assert!(!report.readiness.image_gen_configured);
    }
}

/// 监听 Ctrl+C 与 SIGTERM（Unix 上）；任一触发即返回，axum 进入优雅停机
/// （等待 in-flight 请求完成、不再 accept 新连接）。
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!("install ctrl_c handler failed: {err}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(err) => tracing::error!("install SIGTERM handler failed: {err}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
