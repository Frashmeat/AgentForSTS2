//! ats-web — AgentTheSpire 的 Web 角色 HTTP 服务器。
//!
//! Stage 0：仅暴露 `/api/health` 与静态前端 fallback，作为骨架可运行性证明。
//! 后续 stage 按 `docs/rust-rewrite/roadmap.md` 接入业务路由。

mod static_files;

use axum::{Json, Router, routing::get};
use clap::Parser;
use tower_http::cors::CorsLayer;

#[derive(Debug, Parser)]
#[command(name = "ats-web", version, about = "AgentTheSpire Web server")]
struct Args {
    /// Bind host. Override via `ATS_HOST` env.
    #[arg(long, env = "ATS_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Bind port. Override via `ATS_PORT` env.
    #[arg(long, env = "ATS_PORT", default_value_t = 7860)]
    port: u16,
}

async fn health_handler() -> Json<ats_core::health::HealthReport> {
    Json(ats_core::health::report(ats_core::health::Role::Web))
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

    let api = Router::new().route("/api/health", get(health_handler));

    let app = Router::new()
        .merge(api)
        .fallback(static_files::static_handler)
        .layer(CorsLayer::permissive());

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("ats-web listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
