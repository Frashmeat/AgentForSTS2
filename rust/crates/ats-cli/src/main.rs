//! ats — 部署/打包/日志 CLI，替代 `tools/latest/*.ps1`。
//!
//! Stage 0：仅占位骨架，子命令尚未实现。Stage 7 落地具体调度逻辑
//! （docker compose、模板渲染、签名校验）。

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ats", version, about = "AgentTheSpire deployment CLI")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// 部署应用（对标 tools/latest/deploy-app.ps1）。
    Deploy,
    /// 打包发布物（对标 tools/latest/package-app.ps1）。
    Package,
    /// 查看运行日志（对标 tools/latest/logs-app.ps1）。
    Logs,
    /// 停止运行中的部署（对标 tools/latest/stop-app.ps1）。
    Stop,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Deploy | Cmd::Package | Cmd::Logs | Cmd::Stop => {
            tracing::warn!("subcommand not implemented yet (stage 7)");
            Ok(())
        }
    }
}
