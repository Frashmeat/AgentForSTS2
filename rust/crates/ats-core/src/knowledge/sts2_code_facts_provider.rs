//! STS2 代码事实抽取：扫描反编译产物（game_dir + baselib_decompiled.cs），
//! 用 regex 提取类型/方法/基类符号，按 query 过滤回 `KnowledgeFactItem`。
//!
//! 设计取向：
//! - **regex-only**，不引入完整 C# parser（依赖太重）。只抽 declaration 行，
//!   不解析 method body
//! - 单次 build_facts 调用扫一遍游戏目录（最多 max_files 个文件）
//! - 按 asset_type / symbols / item_name 三个轴过滤，最多返回 `MAX_FACTS` 条
//! - 找不到匹配时退化为返回若干"高价值候选"（带 OverrideMember / hook 关键字
//!   的类型）以免完全空白

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::knowledge::contracts::{KnowledgeFactItem, KnowledgeQuery};
use crate::knowledge::models::SourceMode;
use crate::knowledge::paths::KnowledgePaths;

#[derive(Debug, Default, Clone)]
pub struct Sts2CodeFactsProvider;

/// 单次扫描上限——防止巨型仓库炸 IO。STS2 反编译约几百到几千 .cs，2000 够用。
const MAX_FILES: usize = 2000;
/// 单次返回 fact 上限，避免 prompt 膨胀。
const MAX_FACTS: usize = 12;
/// 单个 fact body 的字符上限。
const MAX_BODY_CHARS: usize = 1200;

impl Sts2CodeFactsProvider {
    /// 返回 (facts, warnings)。
    pub fn build_facts(
        &self,
        query: &KnowledgeQuery,
        paths: &KnowledgePaths,
        game_mode: SourceMode,
    ) -> (Vec<KnowledgeFactItem>, Vec<String>) {
        let mut warnings: Vec<String> = Vec::new();

        if !matches!(game_mode, SourceMode::RuntimeDecompiled) {
            warnings.push(
                "游戏反编译源缺失，无法抽取代码事实；先在知识库面板执行更新。".into(),
            );
            return (Vec::new(), warnings);
        }

        let mut index = CodeFactsIndex::default();
        index.scan_dir(&paths.game_dir, &mut warnings);
        let baselib_file = paths.baselib_decompiled_file();
        if baselib_file.exists() {
            index.scan_file(&baselib_file, &mut warnings);
        } else {
            warnings.push(
                "BaseLib.decompiled.cs 不存在；prompt 不会包含 BaseLib 类型事实。".into(),
            );
        }

        if index.types.is_empty() {
            warnings.push(
                "反编译产物未抽取到任何类型；可能 game_dir 为空或文件格式异常。".into(),
            );
            return (Vec::new(), warnings);
        }

        let facts = index.facts_for_query(query);
        if facts.is_empty() {
            warnings.push(format!(
                "未找到匹配 asset_type={:?} 的类型；共扫描 {} 个 .cs 文件 / {} 个类型符号。",
                query.asset_type,
                index.files_scanned,
                index.types.len()
            ));
        }
        (facts, warnings)
    }
}

// -------- 索引构建 --------

#[derive(Debug, Default)]
struct CodeFactsIndex {
    types: Vec<TypeSymbol>,
    files_scanned: u32,
}

#[derive(Debug, Clone)]
struct TypeSymbol {
    /// 完整名（含 namespace），如 "STS2.Cards.Strike"
    full_name: String,
    /// 简短类型名
    name: String,
    /// "class" / "struct" / "interface" / "enum"
    kind: String,
    /// 文件相对/绝对路径
    file_path: String,
    /// 文件内的起始行（1-based）
    line: u32,
    /// 继承 / 实现的基类型列表
    base_types: Vec<String>,
    /// 同文件抽取的方法签名（最多 8 条），辅助 LLM 看 API surface
    method_signatures: Vec<String>,
    /// 是否包含某些热门关键字（OverrideMember / Patch / 等）—— 用于降级匹配
    has_hook_keyword: bool,
}

impl CodeFactsIndex {
    fn scan_dir(&mut self, dir: &Path, warnings: &mut Vec<String>) {
        if !dir.is_dir() {
            return;
        }
        let walker = walkdir::WalkDir::new(dir).max_depth(8);
        for entry in walker.into_iter().filter_map(Result::ok) {
            if self.files_scanned as usize >= MAX_FILES {
                warnings.push(format!(
                    "已扫描到 MAX_FILES={MAX_FILES} 个 .cs 文件上限，其余被跳过。"
                ));
                return;
            }
            let p = entry.path();
            if p.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("cs"))
            {
                self.scan_file(p, warnings);
            }
        }
    }

    fn scan_file(&mut self, path: &Path, _warnings: &mut Vec<String>) {
        self.files_scanned += 1;
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let display = path.display().to_string();
        let symbols = extract_types(&text, &display);
        self.types.extend(symbols);
    }

    /// 按 query 过滤返回 facts。
    fn facts_for_query(&self, query: &KnowledgeQuery) -> Vec<KnowledgeFactItem> {
        let asset_type_keys: Vec<String> = match &query.asset_type {
            Some(t) if !t.is_empty() => asset_type_keywords(t),
            _ => Vec::new(),
        };
        let symbol_keys: Vec<String> = query
            .symbols
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let item_name_key = query
            .item_name
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());

        let mut scored: Vec<(i32, &TypeSymbol)> = self
            .types
            .iter()
            .map(|t| (score_type(t, &asset_type_keys, &symbol_keys, item_name_key.as_deref()), t))
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.full_name.cmp(&b.1.full_name)));
        scored.truncate(MAX_FACTS);

        scored
            .into_iter()
            .map(|(_, t)| symbol_to_fact(t, &asset_type_keys))
            .collect()
    }
}

// -------- regex 模式（lazy_static 风格通过 OnceLock） --------

fn re_namespace() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // namespace X { 或 namespace X.Y.Z {
        Regex::new(r"(?m)^\s*namespace\s+([A-Za-z_][\w\.]*)\s*\{?").unwrap()
    })
}

fn re_type_decl() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // 至少一个修饰符 + 关键字 + 名字 + 可选泛型 + 可选 base 列表（贪婪到 { / ; / 行尾）。
        // 不锚定到行首，允许 "namespace X { public class Y { } }" 这种单行形态。
        // base 列表里允许 < > 以容纳泛型；调用方负责剥离 "where ..." 子句。
        // rust-regex 不支持 lookahead，所以 base 用 [^{};\n]+ 显式排除终止符。
        Regex::new(
            r"(?:public|internal|private|protected|static|sealed|abstract|partial|unsafe|readonly|record)(?:\s+(?:public|internal|private|protected|static|sealed|abstract|partial|unsafe|readonly|record))*\s+(class|struct|interface|enum)\s+([A-Za-z_]\w*)\s*(?:<[^>]+>)?(?:\s*:\s*([^{};\n]+))?",
        )
        .unwrap()
    })
}

fn re_method_sig() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // 粗匹配：public / protected / private / internal / static / virtual / override /
        // abstract / async + return type + name ( params ) （不含 { ）
        // 排除关键字误判（class / namespace / interface 等）
        Regex::new(
            r"(?m)^\s*(?:(?:public|protected|private|internal|static|virtual|override|abstract|async|sealed|new|extern|unsafe)\s+)+([A-Za-z_][\w<>,\s\[\]\?\.]*?)\s+([A-Za-z_]\w*)\s*\(([^)]*)\)",
        )
        .unwrap()
    })
}

fn re_hook_keyword() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b(OverrideMember|HookManager|Patches?|Hook|RuntimeUtils|Card|Relic|Power|Character)\b").unwrap())
}

// -------- 类型抽取 --------

fn extract_types(text: &str, file_path: &str) -> Vec<TypeSymbol> {
    // 1. 用空行/大括号无法可靠定位 type 边界，采用顺序扫描：
    //    每发现一个 type_decl，把后续 ~100 行内的 method signatures 归到它名下，
    //    直到下一个 type_decl 或文件结束。
    let mut result: Vec<TypeSymbol> = Vec::new();

    // 记录当前 namespace stack
    let mut current_ns: Vec<String> = Vec::new();
    // pending type 在 result 末尾，等其后的方法签名归到它名下
    // (last_type_idx, last_type_line)
    let mut last_type: Option<(usize, u32)> = None;

    for (idx, line) in text.lines().enumerate() {
        let line_num = (idx + 1) as u32;

        // 1. namespace 解析（不 continue：单行 "namespace X { class Y {} }" 形式下，
        //    同一行后续可能还有 type decl）
        if let Some(cap) = re_namespace().captures(line) {
            current_ns.clear();
            current_ns.push(cap[1].to_string());
        }

        // 2. type 声明（regex 不锚行首，能匹配单行嵌套形式）
        if let Some(cap) = re_type_decl().captures(line) {
            let kind = cap[1].to_string();
            let name = cap[2].to_string();
            let base_types: Vec<String> = cap
                .get(3)
                .map(|m| {
                    // 先剥离 "where X : ..." 子句，再 split(',') 拆基类
                    let raw = m
                        .as_str()
                        .split("where")
                        .next()
                        .unwrap_or("")
                        .trim_end_matches('{')
                        .trim();
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let ns = current_ns.join(".");
            let full_name = if ns.is_empty() {
                name.clone()
            } else {
                format!("{ns}.{name}")
            };
            result.push(TypeSymbol {
                full_name,
                name,
                kind,
                file_path: file_path.to_string(),
                line: line_num,
                base_types,
                method_signatures: Vec::new(),
                has_hook_keyword: false,
            });
            last_type = Some((result.len() - 1, line_num));
            continue;
        }

        // 方法签名：归属到最近的 type，且距离不超过 200 行（避免跨文件意外）
        if let Some((idx_in_result, type_line)) = last_type {
            if line_num.saturating_sub(type_line) > 200 {
                continue;
            }
            if let Some(cap) = re_method_sig().captures(line) {
                let return_type = cap[1].trim().to_string();
                let method_name = cap[2].to_string();
                let params = cap[3].trim().to_string();
                let sig = format!("{return_type} {method_name}({params})");
                let t = &mut result[idx_in_result];
                if t.method_signatures.len() < 8 {
                    t.method_signatures.push(sig);
                }
            }
            if re_hook_keyword().is_match(line) {
                result[idx_in_result].has_hook_keyword = true;
            }
        }
    }

    result
}

// -------- 评分 / 过滤 --------

fn asset_type_keywords(asset_type: &str) -> Vec<String> {
    // 把 asset_type 字符串映射到 STS2 类型常见关键字
    let lower = asset_type.to_ascii_lowercase();
    let mut keys: Vec<String> = vec![lower.clone()];
    // 常见对应：card / relic / power / character / custom_code
    let mapped = match lower.as_str() {
        "card" | "card_fullscreen" => vec!["card"],
        "relic" => vec!["relic"],
        "power" => vec!["power"],
        "character" => vec!["character", "player"],
        "custom_code" => vec!["hook", "patch"],
        _ => vec![],
    };
    for m in mapped {
        if !keys.iter().any(|k| k == m) {
            keys.push(m.to_string());
        }
    }
    keys
}

fn score_type(
    t: &TypeSymbol,
    asset_keys: &[String],
    symbol_keys: &[String],
    item_name_key: Option<&str>,
) -> i32 {
    let name_lower = t.name.to_ascii_lowercase();
    let full_lower = t.full_name.to_ascii_lowercase();
    let bases_lower: Vec<String> = t
        .base_types
        .iter()
        .map(|b| b.to_ascii_lowercase())
        .collect();

    let mut score: i32 = 0;

    // asset_type 关键字命中：type 名 +5，base 类型 +8（直接表明用途），namespace +2
    for key in asset_keys {
        if name_lower.contains(key) {
            score += 5;
        }
        if bases_lower.iter().any(|b| b.contains(key)) {
            score += 8;
        }
        if full_lower.contains(key) && !name_lower.contains(key) {
            score += 2;
        }
    }
    // symbols 精确命中 +20
    for key in symbol_keys {
        if name_lower == *key {
            score += 20;
        } else if name_lower.contains(key) {
            score += 6;
        }
    }
    // item_name 命中 +3（弱信号，仅作 tie-breaker）
    if let Some(item_key) = item_name_key {
        if name_lower.contains(item_key) {
            score += 3;
        }
    }
    // hook 关键字降级匹配（asset_keys 为空但有 custom_code 类需求时也能拿到东西）
    if score == 0 && t.has_hook_keyword && asset_keys.iter().any(|k| k == "hook" || k == "patch") {
        score += 1;
    }
    score
}

fn symbol_to_fact(t: &TypeSymbol, asset_keys: &[String]) -> KnowledgeFactItem {
    let title = format!("{} {}", t.kind, t.full_name);
    let mut body_parts: Vec<String> = Vec::new();
    body_parts.push(format!("Declared at line {} of `{}`", t.line, t.file_path));
    if !t.base_types.is_empty() {
        body_parts.push(format!("Inherits: {}", t.base_types.join(", ")));
    }
    if !t.method_signatures.is_empty() {
        body_parts.push("Methods (excerpt):".into());
        for sig in &t.method_signatures {
            body_parts.push(format!("- `{sig}`"));
        }
    }
    let mut body = body_parts.join("\n");
    if body.chars().count() > MAX_BODY_CHARS {
        let head: String = body.chars().take(MAX_BODY_CHARS).collect();
        body = format!("{head}\n...[truncated]");
    }

    let mut keywords: Vec<String> = Vec::new();
    keywords.push(t.name.clone());
    for k in asset_keys {
        if !keywords.iter().any(|x| x.eq_ignore_ascii_case(k)) {
            keywords.push(k.clone());
        }
    }

    let asset_types: Vec<String> = asset_keys.to_vec();

    KnowledgeFactItem {
        key: t.full_name.clone(),
        title,
        body,
        priority: 10, // 后续可基于 score 区分；先统一
        evidence_paths: vec![t.file_path.clone()],
        keywords,
        asset_types,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_source_emits_warning() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery::default(),
            &paths,
            SourceMode::Missing,
        );
        assert!(facts.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("游戏反编译源缺失"));
    }

    fn write_cs(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    #[test]
    fn extracts_namespace_class_and_base_types() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();

        write_cs(
            &paths.game_dir,
            "Cards/StrikeCard.cs",
            r#"using System;

namespace STS2.Cards
{
    public class StrikeCard : AbstractCard
    {
        public override void Use(Player p, Monster m)
        {
            // body
        }

        public void Reset()
        {
            // body
        }
    }
}
"#,
        );

        let query = KnowledgeQuery {
            asset_type: Some("card".into()),
            ..Default::default()
        };
        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &query,
            &paths,
            SourceMode::RuntimeDecompiled,
        );

        assert_eq!(facts.len(), 1, "warnings={warnings:?}");
        let f = &facts[0];
        assert_eq!(f.key, "STS2.Cards.StrikeCard");
        assert!(f.title.contains("class STS2.Cards.StrikeCard"));
        assert!(f.body.contains("AbstractCard"), "body should mention base: {}", f.body);
        assert!(f.body.contains("Use"), "method excerpt missing in body: {}", f.body);
        assert!(f.evidence_paths[0].ends_with("StrikeCard.cs"));
    }

    #[test]
    fn asset_type_relic_matches_base_class() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        write_cs(
            &paths.game_dir,
            "RingOfFireRelic.cs",
            r#"namespace STS2.Relics
{
    public class RingOfFireRelic : Relic
    {
        public override void OnEnterCombat()
        {
        }
    }
}
"#,
        );
        write_cs(
            &paths.game_dir,
            "SomeOther.cs",
            r#"namespace STS2.Utils
{
    public class HelperUtility
    {
    }
}
"#,
        );

        let query = KnowledgeQuery {
            asset_type: Some("relic".into()),
            ..Default::default()
        };
        let (facts, _w) = Sts2CodeFactsProvider.build_facts(
            &query,
            &paths,
            SourceMode::RuntimeDecompiled,
        );

        assert!(
            facts.iter().any(|f| f.key.contains("RingOfFireRelic")),
            "should match Relic by base class; got: {:?}",
            facts.iter().map(|f| &f.key).collect::<Vec<_>>()
        );
        // HelperUtility 无 asset_type 关键字命中，不应进入结果
        assert!(facts.iter().all(|f| !f.key.contains("HelperUtility")));
    }

    #[test]
    fn explicit_symbol_query_takes_precedence() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        write_cs(
            &paths.game_dir,
            "A.cs",
            "namespace STS2 { public class WeirdName { } }",
        );

        let query = KnowledgeQuery {
            symbols: vec!["WeirdName".into()],
            ..Default::default()
        };
        let (facts, _w) = Sts2CodeFactsProvider.build_facts(
            &query,
            &paths,
            SourceMode::RuntimeDecompiled,
        );
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].key, "STS2.WeirdName");
    }

    #[test]
    fn empty_game_dir_warns() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery {
                asset_type: Some("card".into()),
                ..Default::default()
            },
            &paths,
            SourceMode::RuntimeDecompiled,
        );
        assert!(facts.is_empty());
        assert!(
            warnings.iter().any(|w| w.contains("未抽取到任何类型")),
            "warnings={warnings:?}"
        );
    }

    #[test]
    fn baselib_file_is_scanned_if_present() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        // BaseLib 反编译产物是单文件
        fs::write(
            paths.baselib_decompiled_file(),
            "namespace BaseLib { public class HookManager { } }",
        )
        .unwrap();
        // game_dir 留空但加一个 stub 让总不至于完全空
        write_cs(
            &paths.game_dir,
            "Dummy.cs",
            "namespace STS2 { public class DummyType { } }",
        );

        let query = KnowledgeQuery {
            symbols: vec!["HookManager".into()],
            ..Default::default()
        };
        let (facts, _w) = Sts2CodeFactsProvider.build_facts(
            &query,
            &paths,
            SourceMode::RuntimeDecompiled,
        );
        assert!(facts.iter().any(|f| f.key.contains("HookManager")));
    }

    #[test]
    fn missing_baselib_warns_but_does_not_error() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        // 不写 baselib 文件
        write_cs(
            &paths.game_dir,
            "Dummy.cs",
            "namespace STS2 { public class Card { } }",
        );
        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery {
                asset_type: Some("card".into()),
                ..Default::default()
            },
            &paths,
            SourceMode::RuntimeDecompiled,
        );
        assert!(!facts.is_empty());
        assert!(warnings.iter().any(|w| w.contains("BaseLib.decompiled.cs 不存在")));
    }

    #[test]
    fn caps_results_at_max_facts() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        // 写 20 个类型都命中 "card"
        for i in 0..20 {
            write_cs(
                &paths.game_dir,
                &format!("Card{i}.cs"),
                &format!(
                    "namespace STS2.Cards {{ public class Card{i} : AbstractCard {{ }} }}"
                ),
            );
        }
        let (facts, _w) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery {
                asset_type: Some("card".into()),
                ..Default::default()
            },
            &paths,
            SourceMode::RuntimeDecompiled,
        );
        assert_eq!(facts.len(), super::MAX_FACTS);
    }
}
