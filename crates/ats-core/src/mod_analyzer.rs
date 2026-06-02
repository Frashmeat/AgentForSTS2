//! 已存在 mod 项目分析：扫 .csproj / .cs / packages.json / artifacts/，
//! 输出结构化报告供前端做"项目体检"。
//!
//! 不调 LLM，纯文件 IO + 简单文本扫描。预期耗时几十毫秒。

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum ModAnalyzerError {
    #[error("project_root not a directory: {0}")]
    NotADir(PathBuf),
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModAnalysisReport {
    pub project_root: PathBuf,
    /// 主 .csproj 文件路径（取找到的第一个；多 csproj 时优先 Mod / Plugin 命名）
    pub csproj_path: Option<PathBuf>,
    /// .csproj 内 TargetFramework / Sdk / PackageReference 数量等
    pub csproj_summary: Option<CsprojSummary>,
    /// 顶层 packages.json（mod 元信息，若存在）
    pub mod_meta: Option<ModMeta>,
    /// .cs 文件数 + 总字节数
    pub cs_files_count: u32,
    pub cs_total_bytes: u64,
    /// artifacts/ 目录下产出物计数
    pub artifacts_count: u32,
    /// 简短诊断（红 / 黄 / 绿三色提示）
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CsprojSummary {
    pub target_framework: Option<String>,
    pub sdk: Option<String>,
    pub package_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModMeta {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    /// 原始 JSON 文本（截断到 4KB），方便前端调试
    pub raw_excerpt: String,
}

/// 分析项目目录，返回报告。
///
/// # Errors
/// - `NotADir`：project_root 不是目录
/// - `Io`：IO 失败（rare；大多数读错误被吞没并记录为 warning）
pub fn analyze(project_root: &Path) -> Result<ModAnalysisReport, ModAnalyzerError> {
    if !project_root.is_dir() {
        return Err(ModAnalyzerError::NotADir(project_root.to_path_buf()));
    }

    let mut report = ModAnalysisReport {
        project_root: project_root.to_path_buf(),
        ..Default::default()
    };

    // 1. 找 .csproj —— 取找到的第一个；优先名字含 Mod / Plugin / 项目根名的
    let csprojs = find_csprojs(project_root);
    if let Some(main) = pick_main_csproj(&csprojs, project_root) {
        report.csproj_path = Some(main.clone());
        match std::fs::read_to_string(&main) {
            Ok(text) => report.csproj_summary = Some(parse_csproj(&text)),
            Err(err) => report
                .warnings
                .push(format!("read csproj {}: {err}", main.display())),
        }
    } else {
        report.warnings.push("未找到 .csproj 文件。".into());
    }

    // 2. 顶层 packages.json（mod 元数据）
    let mod_meta_path = project_root.join("packages.json");
    if mod_meta_path.is_file() {
        match std::fs::read_to_string(&mod_meta_path) {
            Ok(text) => report.mod_meta = Some(parse_mod_meta(&text)),
            Err(err) => report.warnings.push(format!("read packages.json: {err}")),
        }
    }

    // 3. 扫 .cs 文件
    let (cs_count, cs_bytes) = count_cs_files(project_root);
    report.cs_files_count = cs_count;
    report.cs_total_bytes = cs_bytes;
    if cs_count == 0 {
        report.warnings.push(".cs 源文件数为 0。".into());
    }

    // 4. artifacts/
    let artifacts_dir = project_root.join("artifacts");
    if artifacts_dir.is_dir() {
        report.artifacts_count = WalkDir::new(&artifacts_dir)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count() as u32;
    }

    Ok(report)
}

fn find_csprojs(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csproj"))
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn pick_main_csproj(csprojs: &[PathBuf], project_root: &Path) -> Option<PathBuf> {
    if csprojs.is_empty() {
        return None;
    }
    // 优先名字含 mod / plugin / 项目根 dir 名（不区分大小写）
    let root_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    for c in csprojs {
        let stem = c
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !root_name.is_empty() && stem == root_name.to_ascii_lowercase() {
            return Some(c.clone());
        }
        if stem.contains("mod") || stem.contains("plugin") {
            return Some(c.clone());
        }
    }
    csprojs.first().cloned()
}

fn parse_csproj(text: &str) -> CsprojSummary {
    let mut s = CsprojSummary::default();
    // <PropertyGroup ... Sdk="..."> 或顶层 <Project Sdk="...">
    if let Some(cap) = Regex::new(r#"<Project[^>]*\sSdk="([^"]+)""#)
        .unwrap()
        .captures(text)
    {
        s.sdk = Some(cap[1].to_string());
    }
    if let Some(cap) = Regex::new(r"<TargetFramework>([^<]+)</TargetFramework>")
        .unwrap()
        .captures(text)
    {
        s.target_framework = Some(cap[1].trim().to_string());
    }
    let pkg_re = Regex::new(r#"<PackageReference\s+Include="([^"]+)""#).unwrap();
    for cap in pkg_re.captures_iter(text) {
        s.package_references.push(cap[1].to_string());
    }
    s
}

fn parse_mod_meta(text: &str) -> ModMeta {
    let mut m = ModMeta::default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        m.name = value
            .get("name")
            .or_else(|| value.get("modName"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        m.author = value
            .get("author")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        m.version = value
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    let head: String = text.chars().take(4000).collect();
    m.raw_excerpt = head;
    m
}

fn count_cs_files(root: &Path) -> (u32, u64) {
    let mut count: u32 = 0;
    let mut bytes: u64 = 0;
    for entry in WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if entry.file_type().is_file()
            && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cs"))
        {
            count += 1;
            if let Ok(meta) = entry.metadata() {
                bytes += meta.len();
            }
        }
    }
    (count, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_at(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn analyze_rejects_non_dir() {
        let td = tempfile::TempDir::new().unwrap();
        let file_path = td.path().join("file.txt");
        fs::write(&file_path, b"x").unwrap();
        let err = analyze(&file_path).unwrap_err();
        assert!(matches!(err, ModAnalyzerError::NotADir(_)));
    }

    #[test]
    fn analyze_extracts_csproj_summary() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path();
        write_at(
            root,
            "DemoMod.csproj",
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.1" />
    <PackageReference Include="BepInEx.Core" Version="6.0.0" />
  </ItemGroup>
</Project>"#,
        );
        write_at(root, "DemoMod.cs", "public class DemoMod { }");

        let report = analyze(root).unwrap();
        assert!(report.csproj_path.is_some());
        let s = report.csproj_summary.unwrap();
        assert_eq!(s.sdk.as_deref(), Some("Microsoft.NET.Sdk"));
        assert_eq!(s.target_framework.as_deref(), Some("net9.0"));
        assert_eq!(s.package_references.len(), 2);
        assert!(s.package_references.contains(&"Newtonsoft.Json".into()));
        assert!(s.package_references.contains(&"BepInEx.Core".into()));
        assert_eq!(report.cs_files_count, 1);
    }

    #[test]
    fn analyze_picks_mod_named_csproj_when_multiple() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path();
        write_at(root, "Helpers.csproj", "<Project></Project>");
        write_at(root, "DemoMod.csproj", "<Project></Project>");
        write_at(root, "Tests.csproj", "<Project></Project>");
        let report = analyze(root).unwrap();
        let main = report.csproj_path.unwrap();
        assert!(
            main.file_stem().unwrap().to_string_lossy().contains("Mod"),
            "expected Mod-named csproj; got {}",
            main.display()
        );
    }

    #[test]
    fn analyze_parses_packages_json() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path();
        write_at(
            root,
            "packages.json",
            r#"{"name":"my_mod","author":"alice","version":"1.2.3"}"#,
        );
        let report = analyze(root).unwrap();
        let m = report.mod_meta.expect("mod_meta");
        assert_eq!(m.name.as_deref(), Some("my_mod"));
        assert_eq!(m.author.as_deref(), Some("alice"));
        assert_eq!(m.version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn analyze_warns_when_no_csproj_no_cs() {
        let td = tempfile::TempDir::new().unwrap();
        let report = analyze(td.path()).unwrap();
        assert!(
            report.warnings.iter().any(|w| w.contains("csproj")),
            "expected csproj warning; got {:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains(".cs 源文件数为 0")),
            "expected cs-files warning"
        );
    }

    #[test]
    fn analyze_counts_artifacts() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path();
        fs::create_dir_all(root.join("artifacts/Card1")).unwrap();
        fs::write(root.join("artifacts/Card1/Card1.cs"), b"// a").unwrap();
        fs::write(root.join("artifacts/Card1/Card1.png"), b"png").unwrap();
        fs::write(root.join("artifacts/raw.md"), b"raw").unwrap();
        let report = analyze(root).unwrap();
        assert_eq!(report.artifacts_count, 3);
    }
}
