//! 运行时审计日志 —— 追加式 JSONL，写到 `<project>/.ats/audit.log`。
//!
//! 用途：用户排查"上次 LLM 跑了什么 / 哪个任务失败 / 凭据用哪个 model" 等历史问题。
//! 单条记录是 newline-delimited JSON（每行一个 AuditEntry），方便 grep / 增量追加。
//!
//! 抽象层：`AuditSink` trait + 三个实现：
//! - `FileAuditSink`：按工程目录写 `.ats/audit.log`（生产）
//! - `NoopAuditSink`：忽略所有事件（测试 / 没有 active project 的场景）
//! - 自动接线在 `AuditedJobRepository` —— 任意 JobRepository 都可被包一层
//!   自动 emit 状态迁移事件，handlers 完全无感知。

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    /// 事件类型，例 "job.submitted" / "job.completed" / "knowledge.refresh"
    pub kind: String,
    /// 一句话摘要
    pub message: String,
    /// 可选业务 ID（job_id / item_id / plan_id 等）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    /// 可选附加结构化数据
    #[serde(default)]
    pub data: serde_json::Value,
}

impl AuditEntry {
    #[must_use]
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            kind: kind.into(),
            message: message.into(),
            ref_id: None,
            data: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_ref(mut self, id: impl Into<String>) -> Self {
        self.ref_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse line {line_num}: {message}")]
    Parse { line_num: usize, message: String },
}

/// 写入到 `<project>/.ats/audit.log`（追加模式）。父目录不存在会自动创建。
///
/// 写入用 OpenOptions + create(true) + append(true)。O_APPEND 让每次 write(2)
/// 都定位到文件末尾；单行 JSON 通常小于一次 write 的原子上限，实践中不交错，
/// 但 write_all 可能拆成多次 write，并非硬性原子保证（高并发巨行时可能交错）。
pub fn append_entry(project_root: &Path, entry: &AuditEntry) -> Result<(), AuditError> {
    let log_path = audit_log_path(project_root);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AuditError::Io(e.to_string()))?;
    }
    let mut line =
        serde_json::to_string(entry).map_err(|e| AuditError::Io(format!("serialize: {e}")))?;
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| AuditError::Io(format!("open {}: {e}", log_path.display())))?;
    f.write_all(line.as_bytes())
        .map_err(|e| AuditError::Io(e.to_string()))?;
    Ok(())
}

/// 读取最近 N 条审计记录（按文件物理顺序倒序：最新在前）。
pub fn read_recent(project_root: &Path, limit: usize) -> Result<Vec<AuditEntry>, AuditError> {
    let log_path = audit_log_path(project_root);
    if !log_path.is_file() {
        return Ok(Vec::new());
    }
    let f = std::fs::File::open(&log_path).map_err(|e| AuditError::Io(e.to_string()))?;
    let reader = BufReader::new(f);
    let mut all: Vec<AuditEntry> = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| AuditError::Io(e.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(&line).map_err(|e| AuditError::Parse {
            line_num: i + 1,
            message: e.to_string(),
        })?;
        all.push(entry);
    }
    if all.len() > limit {
        all = all.split_off(all.len() - limit);
    }
    all.reverse();
    Ok(all)
}

pub fn audit_log_path(project_root: &Path) -> PathBuf {
    project_root.join(".ats").join("audit.log")
}

/// 异步审计写入抽象。生产用 [`FileAuditSink`]；测试 / 无 project 场景用
/// [`NoopAuditSink`]；observer 装配通过 [`AuditedJobRepository`] 在 JobRepository
/// 状态迁移点自动 emit，handlers 不感知。
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn emit(&self, entry: AuditEntry);
}

/// 不做任何事的 sink。常用于 unit test 或没有 active project 的情境。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuditSink;

#[async_trait]
impl AuditSink for NoopAuditSink {
    async fn emit(&self, _entry: AuditEntry) {}
}

/// 把 entry 追加到 `<project_root>/.ats/audit.log`。
/// I/O 通过 spawn_blocking 包住——append 是同步系统调用，避免阻塞 tokio 调度。
/// 写失败只 eprintln（审计不是关键路径，不能影响业务任务的成功/失败判定）。
#[derive(Debug, Clone)]
pub struct FileAuditSink {
    project_root: PathBuf,
}

impl FileAuditSink {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[async_trait]
impl AuditSink for FileAuditSink {
    async fn emit(&self, entry: AuditEntry) {
        let root = self.project_root.clone();
        let result = tokio::task::spawn_blocking(move || append_entry(&root, &entry)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => eprintln!("audit emit failed: {err}"),
            Err(join) => eprintln!("audit emit join error: {join}"),
        }
    }
}

/// 类型别名，统一 trait object Arc 写法（替代到处写 `Arc<dyn AuditSink>`）。
pub type AuditSinkArc = Arc<dyn AuditSink>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_then_read_round_trips() {
        let td = tempfile::TempDir::new().unwrap();
        let entry1 = AuditEntry::new("job.submitted", "code_generate started").with_ref("job-1");
        let entry2 = AuditEntry::new("job.completed", "code_generate ok")
            .with_ref("job-1")
            .with_data(serde_json::json!({"csPath": "/foo/Bar.cs"}));
        append_entry(td.path(), &entry1).unwrap();
        append_entry(td.path(), &entry2).unwrap();

        let entries = read_recent(td.path(), 10).unwrap();
        assert_eq!(entries.len(), 2);
        // 最新在前
        assert_eq!(entries[0].kind, "job.completed");
        assert_eq!(entries[0].ref_id.as_deref(), Some("job-1"));
        assert_eq!(entries[0].data["csPath"], "/foo/Bar.cs");
        assert_eq!(entries[1].kind, "job.submitted");
    }

    #[test]
    fn read_returns_empty_when_no_log() {
        let td = tempfile::TempDir::new().unwrap();
        let entries = read_recent(td.path(), 10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_respects_limit() {
        let td = tempfile::TempDir::new().unwrap();
        for i in 0..20 {
            append_entry(td.path(), &AuditEntry::new("test", format!("entry {i}"))).unwrap();
        }
        let last_5 = read_recent(td.path(), 5).unwrap();
        assert_eq!(last_5.len(), 5);
        // 最新在前：entry 19, 18, 17, 16, 15
        assert!(last_5[0].message.ends_with("19"));
        assert!(last_5[4].message.ends_with("15"));
    }

    #[test]
    fn parse_errors_in_corrupted_lines() {
        let td = tempfile::TempDir::new().unwrap();
        let p = audit_log_path(td.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"{\"timestamp\":\"2026-05-11T00:00:00Z\",\"kind\":\"ok\",\"message\":\"a\"}\nNOT JSON\n").unwrap();
        let err = read_recent(td.path(), 10).unwrap_err();
        assert!(matches!(err, AuditError::Parse { line_num: 2, .. }));
    }
}
