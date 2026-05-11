//! ats — AgentTheSpire 部署 / 打包 / 日志 CLI。
//!
//! 当前实现：
//! - `build`：实际跑 `cargo build --release -p ats-web` 或 Tauri bundle
//! - `deploy` / `logs` / `stop`：Docker compose 子命令（需要用户自带
//!   compose.app.yml；提供模板提示而非生成）
//!
//! 设计取向：本 CLI 不维护进程，只调度外部工具。
//! Docker compose 模板内嵌见 stage 7 后续 commit。

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ats", version, about = "AgentTheSpire deployment CLI")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// 构建产物：cargo + frontend + 可选 Tauri bundle。
    Build {
        /// 同时构建桌面 Tauri 安装包（需要本机已装 tauri-cli）
        #[arg(long)]
        desktop: bool,
        /// 同时构建 Web 端 binary（cargo build --release -p ats-web）
        #[arg(long)]
        web: bool,
    },
    /// 部署应用（docker compose up）。需要用户自带 compose.app.yml。
    Deploy {
        /// compose 文件路径（默认 ./compose.app.yml）
        #[arg(long, default_value = "compose.app.yml")]
        compose: PathBuf,
    },
    /// 查看运行日志（docker compose logs -f）。
    Logs {
        #[arg(long, default_value = "compose.app.yml")]
        compose: PathBuf,
        /// 跟随后续日志（-f）
        #[arg(long, default_value_t = true)]
        follow: bool,
    },
    /// 停止部署（docker compose down）。
    Stop {
        #[arg(long, default_value = "compose.app.yml")]
        compose: PathBuf,
    },
    /// 打印 Docker compose.app.yml 推荐模板（用户自己重定向到文件再改）。
    PrintComposeTemplate,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::Build { desktop, web } => cmd_build(desktop, web),
        Cmd::Deploy { compose } => cmd_compose("up", "-d", &compose),
        Cmd::Logs { compose, follow } => cmd_logs(&compose, follow),
        Cmd::Stop { compose } => cmd_compose("down", "", &compose),
        Cmd::PrintComposeTemplate => {
            print!("{}", COMPOSE_TEMPLATE);
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ats: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_build(desktop: bool, web: bool) -> Result<()> {
    // 默认 (--web --desktop 都不指定) 跑前端 build
    let do_web = web || !desktop;
    let do_desktop = desktop;

    if do_web {
        tracing::info!("building web frontend (vite)");
        run("npm", &["run", "build:web"], None)?;
        tracing::info!("building ats-web release binary");
        run(
            "cargo",
            &["build", "--release", "-p", "ats-web"],
            None,
        )?;
    }
    if do_desktop {
        tracing::info!("building Tauri desktop bundle");
        run("npm", &["run", "tauri", "build"], None)?;
    }
    Ok(())
}

fn cmd_compose(action: &str, extra: &str, compose: &Path) -> Result<()> {
    if !compose.is_file() {
        anyhow::bail!(
            "compose file not found: {}\n  hint: run `ats print-compose-template > compose.app.yml` 先生成一个模板",
            compose.display()
        );
    }
    let mut args = vec!["compose", "-f", compose.to_str().unwrap(), action];
    if !extra.is_empty() {
        args.push(extra);
    }
    run("docker", &args, None)
}

fn cmd_logs(compose: &Path, follow: bool) -> Result<()> {
    if !compose.is_file() {
        anyhow::bail!("compose file not found: {}", compose.display());
    }
    let mut args = vec!["compose", "-f", compose.to_str().unwrap(), "logs"];
    if follow {
        args.push("-f");
    }
    run("docker", &args, None)
}

fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    tracing::debug!(program, ?args, "spawning");
    let status = cmd
        .status()
        .with_context(|| format!("spawn {program}"))?;
    if !status.success() {
        anyhow::bail!(
            "{program} {} exited with {}",
            args.join(" "),
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "(no code)".into())
        );
    }
    Ok(())
}

/// 推荐的 docker compose 模板（用户自己改 image / volumes / ports）。
const COMPOSE_TEMPLATE: &str = r#"# AgentTheSpire — docker compose 推荐模板
# 用 `ats print-compose-template > compose.app.yml` 生成一份，再按需修改。
#
# 假设：你已经构建好了 ats-web 镜像（或用本仓库根的 Dockerfile）。
#
services:
  ats-web:
    image: agentthespire/ats-web:latest
    container_name: ats-web
    restart: unless-stopped
    ports:
      - "7870:7870"
    environment:
      - RUST_LOG=info
      - SPIREFORGE_LLM__API_KEY=${LLM_API_KEY}
      - SPIREFORGE_LLM__BASE_URL=${LLM_BASE_URL:-}
      - SPIREFORGE_LLM__MODEL=${LLM_MODEL:-claude-sonnet-4-6}
    volumes:
      - ./runtime:/app/runtime
      # 如果你想内嵌 rembg ONNX 模型，挂卷到 /app/models
      # - ./models:/app/models:ro
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:7870/api/health"]
      interval: 30s
      timeout: 5s
      retries: 3
"#;
