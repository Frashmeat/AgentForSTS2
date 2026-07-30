//! 工程脚手架：从已验证 Game Pack 的稳定模板创建项目。
//!
//! - Pack loader 在脚手架执行前验证完整模板目录树 SHA-256
//! - 文件名 / 文件内容中的 Pack placeholder 替换为派生的 csharp_name
//! - 二进制文件按字节原样落盘；文本文件先 UTF-8 解码再替换

use std::path::{Path, PathBuf};

use super::error::{ProjectError, ProjectResult};
use crate::game_pack::{JsonManifestContract, LoadedGamePack, ProjectTemplate};

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

/// 把 Game Pack 模板完整拷到 `project_root`，并替换文件名与 UTF-8 内容中的 placeholder。
///
/// `project_root` 必须已经存在；不存在则报 `ProjectError::Missing`。
pub fn scaffold_from_template(
    project_root: &Path,
    csharp_name: &str,
    pack: &LoadedGamePack,
) -> ProjectResult<()> {
    if !project_root.is_dir() {
        return Err(ProjectError::Missing(project_root.display().to_string()));
    }
    let template = pack.project_template.as_ref().ok_or_else(|| {
        ProjectError::ScaffoldContract(format!(
            "game pack `{}` does not declare a project template",
            pack.id
        ))
    })?;
    write_template_files(template, project_root, csharp_name)?;
    if let Some(contract) = &pack.manifest_contract {
        validate_manifest_contract(project_root, csharp_name, contract)?;
    }
    Ok(())
}

fn write_template_files(
    template: &ProjectTemplate,
    target_root: &Path,
    csharp_name: &str,
) -> ProjectResult<()> {
    for file in &template.files {
        let rel = Path::new(&file.relative_path);
        let rel_renamed = rename_path_components(rel, &template.placeholder, csharp_name);
        let abs = target_root.join(rel_renamed);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Ok(text) = std::str::from_utf8(&file.bytes) {
            let replaced = text.replace(&template.placeholder, csharp_name);
            std::fs::write(&abs, replaced.as_bytes())?;
        } else {
            std::fs::write(&abs, &file.bytes)?;
        }
    }
    Ok(())
}

fn rename_path_components(rel: &Path, placeholder: &str, csharp_name: &str) -> PathBuf {
    rel.components()
        .map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.contains(placeholder) {
                PathBuf::from(s.replace(placeholder, csharp_name))
            } else {
                PathBuf::from(s.into_owned())
            }
        })
        .fold(PathBuf::new(), |acc, p| acc.join(p))
}

fn validate_manifest_contract(
    project_root: &Path,
    csharp_name: &str,
    contract: &JsonManifestContract,
) -> ProjectResult<()> {
    let relative = contract.relative_path.replace("{mod_id}", csharp_name);
    let path = project_root.join(relative);
    let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let expected = render_json_placeholders(&contract.expected, csharp_name);
    if actual != expected {
        return Err(ProjectError::ScaffoldContract(format!(
            "manifest `{}` differs from the Pack's expected JSON value",
            path.display()
        )));
    }
    Ok(())
}

fn render_json_placeholders(value: &serde_json::Value, csharp_name: &str) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(text.replace("{mod_id}", csharp_name))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .iter()
                .map(|value| render_json_placeholders(value, csharp_name))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.replace("{mod_id}", csharp_name),
                        render_json_placeholders(value, csharp_name),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_pack::{GamePackLoadPolicy, GamePackLoader, GamePackRegistry};
    use sha2::{Digest, Sha256};

    fn sts2_pack() -> LoadedGamePack {
        GamePackRegistry::built_in()
            .unwrap()
            .require("sts2")
            .unwrap()
            .clone()
    }

    #[test]
    fn scaffold_is_driven_by_a_non_sts2_pack_fixture() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack_root = temp.path().join("fixture-pack");
        let template_root = pack_root.join("template");
        std::fs::create_dir_all(&template_root).unwrap();
        let bytes = br#"{"id":"FixtureName"}"#;
        std::fs::write(template_root.join("FixtureName.json"), bytes).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(b"FixtureName.json");
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(Sha256::digest(bytes));
        let tree_sha = format!("{:x}", hasher.finalize());
        let manifest = serde_json::json!({
            "schema_version": 1,
            "id": "fixture-game",
            "display_name": "Fixture Game",
            "capabilities": ["project_template", "manifest_contract"],
            "project_template": {
                "root": "template",
                "placeholder": "FixtureName",
                "sha256": tree_sha,
                "files": ["FixtureName.json"]
            },
            "manifest_contract": {
                "relative_path": "{mod_id}.json",
                "expected": {"id": "{mod_id}"}
            }
        });
        std::fs::write(
            pack_root.join("game-pack.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let pack = GamePackLoader::new(GamePackLoadPolicy::new(
            ["project_template", "manifest_contract"],
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        ))
        .load_from_dir(&pack_root)
        .unwrap();
        let project_root = temp.path().join("project");
        std::fs::create_dir(&project_root).unwrap();

        scaffold_from_template(&project_root, "OtherMod", &pack).unwrap();

        let actual = std::fs::read_to_string(project_root.join("OtherMod.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&actual).unwrap(),
            serde_json::json!({"id":"OtherMod"})
        );
    }

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

        scaffold_from_template(&root, "MyMod", &sts2_pack()).expect("scaffold");

        let csproj = root.join("MyMod.csproj");
        assert!(csproj.is_file(), "csproj not found: {}", csproj.display());
        let csproj_text = std::fs::read_to_string(&csproj).unwrap();
        assert!(
            !csproj_text.contains("ModTemplate"),
            "csproj still contains ModTemplate placeholder"
        );
        assert!(csproj_text.contains("MyMod"));
        assert!(csproj_text.contains(
            r#"<PackageReference Include="Alchyr.Sts2.BaseLib" Version="3.3.8" PrivateAssets="All" />"#
        ));
        assert!(csproj_text.contains(
            r#"<PackageReference Include="Alchyr.Sts2.ModAnalyzers" Version="0.1.9" />"#
        ));
        assert!(!csproj_text.contains("Version=\"*\""));

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

        scaffold_from_template(&root, "Alpha", &sts2_pack()).expect("scaffold");

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

        scaffold_from_template(&root, "Beta", &sts2_pack()).expect("scaffold");

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

        scaffold_from_template(&root, "Gamma", &sts2_pack()).expect("scaffold");

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

        scaffold_from_template(&root, "ExportSafe", &sts2_pack()).expect("scaffold");

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
        let err = scaffold_from_template(&nope, "X", &sts2_pack()).unwrap_err();
        assert!(matches!(err, ProjectError::Missing(_)));
    }
}
