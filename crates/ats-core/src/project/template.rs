//! 工程脚手架：用仓库根的 `mod_template/` 在新建工程时铺一份 dotnet 项目骨架。
//!
//! - 文件通过 `include_dir!` 在编译期嵌入二进制
//! - 文件名 / 文件内容中的 `ModTemplate` 字面量替换为派生的 csharp_name
//! - 二进制文件按字节原样落盘；文本文件先 UTF-8 解码再替换
//!
//! 文件名规则：含 `ModTemplate` 子串的文件会按相同规则改名
//! （例：`ModTemplate.csproj` → `<name>.csproj`、`ModTemplate.sln.DotSettings`
//! → `<name>.sln.DotSettings`）。

use std::path::Path;

use include_dir::{Dir, include_dir};

use super::error::{ProjectError, ProjectResult};

/// 模板根目录，编译期嵌入。注意路径是相对当前 crate 根的（即 crates/ats-core）。
static MOD_TEMPLATE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../mod_template");

const PLACEHOLDER: &str = "ModTemplate";

/// 从项目显示名派生一个 C# 合法标识符。
///
/// - 非 ASCII 字母数字 / 下划线 → `_`
/// - 首字符是数字 → 前缀 `Mod`
/// - 空字符串兜底为 `MyMod`
#[must_use]
pub fn derive_csharp_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        return "MyMod".into();
    }
    if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("Mod{trimmed}");
    }
    trimmed
}

/// 把 `mod_template/*` 完整拷到 `project_root`，把所有 `ModTemplate` 字面量
/// 替换成 `csharp_name`（文件名与内容）。
///
/// `project_root` 必须已经存在；不存在则报 `ProjectError::Missing`。
pub fn scaffold_from_template(project_root: &Path, csharp_name: &str) -> ProjectResult<()> {
    if !project_root.is_dir() {
        return Err(ProjectError::Missing(project_root.display().to_string()));
    }
    write_recursive(&MOD_TEMPLATE_DIR, project_root, csharp_name)
}

fn write_recursive(dir: &Dir<'_>, target_root: &Path, csharp_name: &str) -> ProjectResult<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => {
                let rel = sub.path();
                let rel_renamed = rename_path_components(rel, csharp_name);
                let abs = target_root.join(&rel_renamed);
                std::fs::create_dir_all(&abs)?;
                write_recursive(sub, target_root, csharp_name)?;
            }
            include_dir::DirEntry::File(f) => {
                let rel = f.path();
                let rel_renamed = rename_path_components(rel, csharp_name);
                let abs = target_root.join(&rel_renamed);
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let bytes = f.contents();
                if let Ok(text) = std::str::from_utf8(bytes) {
                    let replaced = text.replace(PLACEHOLDER, csharp_name);
                    std::fs::write(&abs, replaced.as_bytes())?;
                } else {
                    std::fs::write(&abs, bytes)?;
                }
            }
        }
    }
    Ok(())
}

fn rename_path_components(rel: &Path, csharp_name: &str) -> std::path::PathBuf {
    rel.components()
        .map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.contains(PLACEHOLDER) {
                std::path::PathBuf::from(s.replace(PLACEHOLDER, csharp_name))
            } else {
                std::path::PathBuf::from(s.into_owned())
            }
        })
        .fold(std::path::PathBuf::new(), |acc, p| acc.join(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_csharp_name_ascii_passthrough() {
        assert_eq!(derive_csharp_name("MyMod"), "MyMod");
        assert_eq!(derive_csharp_name("my_mod"), "my_mod");
    }

    #[test]
    fn derive_csharp_name_replaces_invalid() {
        assert_eq!(derive_csharp_name("my mod 1"), "my_mod_1");
    }

    #[test]
    fn derive_csharp_name_strips_chinese_with_fallback() {
        // 全中文 → 全部变 _ → trim 后为空 → fallback
        assert_eq!(derive_csharp_name("我的模组"), "MyMod");
    }

    #[test]
    fn derive_csharp_name_prefixes_leading_digit() {
        assert_eq!(derive_csharp_name("1stMod"), "Mod1stMod");
    }

    #[test]
    fn scaffold_writes_csproj_and_mainfile() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();

        scaffold_from_template(&root, "MyMod").expect("scaffold");

        let csproj = root.join("MyMod.csproj");
        assert!(csproj.is_file(), "csproj not found: {}", csproj.display());
        let csproj_text = std::fs::read_to_string(&csproj).unwrap();
        assert!(
            !csproj_text.contains("ModTemplate"),
            "csproj still contains ModTemplate placeholder"
        );
        assert!(csproj_text.contains("MyMod"));

        let mainfile = root.join("MainFile.cs");
        assert!(mainfile.is_file());
        let mainfile_text = std::fs::read_to_string(&mainfile).unwrap();
        assert!(mainfile_text.contains("namespace MyMod;"));
        assert!(mainfile_text.contains("ModId = \"MyMod\""));
    }

    #[test]
    fn scaffold_writes_sln_with_renamed_project() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path().join("Alpha");
        std::fs::create_dir_all(&root).unwrap();

        scaffold_from_template(&root, "Alpha").expect("scaffold");

        let sln = root.join("Alpha.sln");
        assert!(sln.is_file());
        let sln_text = std::fs::read_to_string(&sln).unwrap();
        assert!(sln_text.contains("Alpha.csproj"));
        assert!(!sln_text.contains("ModTemplate"));
    }

    #[test]
    fn scaffold_writes_extensions_subdir() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path().join("Beta");
        std::fs::create_dir_all(&root).unwrap();

        scaffold_from_template(&root, "Beta").expect("scaffold");

        let ext = root.join("Extensions").join("StringExtensions.cs");
        assert!(ext.is_file(), "{} missing", ext.display());
        let ext_text = std::fs::read_to_string(&ext).unwrap();
        assert!(ext_text.contains("namespace Beta.Extensions"));
        assert!(!ext_text.contains("ModTemplate"));
    }

    #[test]
    fn scaffold_writes_mod_metadata_json() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path().join("Gamma");
        std::fs::create_dir_all(&root).unwrap();

        scaffold_from_template(&root, "Gamma").expect("scaffold");

        let json = root.join("Gamma.json");
        assert!(json.is_file());
        let txt = std::fs::read_to_string(&json).unwrap();
        let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["id"], "Gamma");
        assert_eq!(v["name"], "Gamma");
        assert_eq!(v["min_game_version"], "0.107.1");

        let dependencies = v["dependencies"]
            .as_array()
            .expect("dependencies must use the current object schema");
        assert_eq!(dependencies.len(), 1);
        assert!(
            dependencies.iter().all(serde_json::Value::is_object),
            "old string-only dependency entries are not an accepted scaffold baseline"
        );
        assert_eq!(dependencies[0]["id"], "BaseLib");
        assert_eq!(dependencies[0]["min_version"], "v3.3.8");
    }

    #[test]
    fn scaffold_limits_pck_export_to_runtime_resources() {
        let td = tempfile::TempDir::new().unwrap();
        let root = td.path().join("ExportSafe");
        std::fs::create_dir_all(&root).unwrap();

        scaffold_from_template(&root, "ExportSafe").expect("scaffold");

        let preset = std::fs::read_to_string(root.join("export_presets.cfg")).unwrap();
        assert!(preset.contains(r#"include_filter="ExportSafe/**/*.json""#));
        assert!(!preset.contains(r#"include_filter="*.json""#));
        for excluded in [
            ".ats/**",
            "artifacts/**",
            "history/**",
            "items/**",
            "packages/**",
            "Generated/**",
            "project.json",
        ] {
            assert!(
                preset.contains(excluded),
                "missing PCK exclusion: {excluded}"
            );
        }
    }

    #[test]
    fn scaffold_fails_when_target_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let nope = td.path().join("does_not_exist");
        let err = scaffold_from_template(&nope, "X").unwrap_err();
        assert!(matches!(err, ProjectError::Missing(_)));
    }
}
