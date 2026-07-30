//! STS2 代码事实抽取：扫描反编译产物（game_dir + baselib_decompiled.cs），
//! 用 regex 提取类型/方法/基类符号，按 query 过滤回 `KnowledgeFactItem`。
//!
//! 设计取向：
//! - **regex-only**，不引入完整 C# parser（依赖太重）。只抽 declaration 行，
//!   不解析 method body
//! - 单次 build_facts 调用确定性地扫描游戏目录（受安全上限保护）
//! - 按 asset_type / symbols / item_name 三个轴过滤，最多返回 `MAX_FACTS` 条声明事实；
//!   需求相关的官方实现与生命周期证据另行追加
//! - 找不到匹配时退化为返回若干"高价值候选"（带 OverrideMember / hook 关键字
//!   的类型）以免完全空白

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use thiserror::Error;

use crate::game_pack::VerifiedTruthSnapshot;
use crate::knowledge::contracts::{KnowledgeFactItem, KnowledgeQuery};
#[cfg(test)]
use crate::knowledge::models::SourceMode;
#[cfg(test)]
use crate::knowledge::paths::KnowledgePaths;

#[derive(Debug, Default, Clone)]
pub struct Sts2CodeFactsProvider;

#[derive(Debug, Error)]
pub enum SnapshotCodeFactsError {
    #[error("verified truth snapshot `{snapshot_id}` has no indexes for provider `{provider}`")]
    MissingProvider {
        snapshot_id: String,
        provider: String,
    },
}

/// 单次扫描安全上限。当前完整 STS2 反编译约 3,425 个 .cs 文件，保留足够余量。
const MAX_FILES: usize = 10_000;
/// 单次返回 fact 上限，避免 prompt 膨胀。
const MAX_FACTS: usize = 12;
/// 单个 fact body 的字符上限。
const MAX_BODY_CHARS: usize = 1200;
/// 行为证据优先按 needle 顺序取样，限制每个 needle 和总匹配数，避免大型源码撑爆 prompt。
const MAX_EVIDENCE_MATCHES_PER_NEEDLE: usize = 3;
const MAX_EVIDENCE_RANGES: usize = 12;

impl Sts2CodeFactsProvider {
    /// 返回 (facts, warnings)。
    #[cfg(test)]
    pub fn build_facts(
        &self,
        query: &KnowledgeQuery,
        paths: &KnowledgePaths,
        game_mode: SourceMode,
    ) -> (Vec<KnowledgeFactItem>, Vec<String>) {
        let mut warnings: Vec<String> = Vec::new();

        if !matches!(game_mode, SourceMode::RuntimeDecompiled) {
            warnings.push("游戏反编译源缺失，无法抽取代码事实；先在知识库面板执行更新。".into());
            return (Vec::new(), warnings);
        }

        let mut index = CodeFactsIndex::default();
        index.scan_dir(&paths.game_dir, &mut warnings);
        let baselib_file = paths.baselib_decompiled_file();
        if baselib_file.exists() {
            index.scan_file(&baselib_file, &mut warnings);
        } else {
            warnings
                .push("BaseLib.decompiled.cs 不存在；prompt 不会包含 BaseLib 类型事实。".into());
        }

        finish_facts(query, index, warnings)
    }

    /// Query all snapshot indexes assigned to one provider as a single fact corpus.
    ///
    /// Grouping before ranking preserves the legacy behavior where game and
    /// BaseLib symbols share one `MAX_FACTS` budget and one evidence selection.
    pub fn build_facts_from_snapshot(
        &self,
        query: &KnowledgeQuery,
        snapshot: &VerifiedTruthSnapshot,
        provider: &str,
    ) -> Result<(Vec<KnowledgeFactItem>, Vec<String>), SnapshotCodeFactsError> {
        let provider_indexes = snapshot.provider_index_roots(provider);
        if provider_indexes.is_empty() {
            return Err(SnapshotCodeFactsError::MissingProvider {
                snapshot_id: snapshot.snapshot_id().into(),
                provider: provider.into(),
            });
        }

        let mut warnings = Vec::new();
        let mut index = CodeFactsIndex::default();
        for (manifest, root) in provider_indexes {
            let logical_root = format!(
                "snapshot://{}/{}/",
                snapshot.snapshot_id(),
                manifest.source_id
            );
            index.scan_dir_with_prefix(&root, &logical_root, &mut warnings);
        }
        Ok(finish_facts(query, index, warnings))
    }
}

fn finish_facts(
    query: &KnowledgeQuery,
    index: CodeFactsIndex,
    mut warnings: Vec<String>,
) -> (Vec<KnowledgeFactItem>, Vec<String>) {
    if index.types.is_empty() {
        warnings.push("反编译产物未抽取到任何类型；可能 game_dir 为空或文件格式异常。".into());
        return (Vec::new(), warnings);
    }

    let mut facts = index.behavior_evidence_for_query(query);
    facts.extend(index.facts_for_query(query));
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

// -------- 索引构建 --------

#[derive(Debug, Default)]
struct CodeFactsIndex {
    types: Vec<TypeSymbol>,
    source_files: Vec<SourceFile>,
    files_scanned: u32,
}

#[derive(Debug, Clone)]
struct SourceFile {
    path: String,
    text: String,
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
    #[cfg(test)]
    fn scan_dir(&mut self, dir: &Path, warnings: &mut Vec<String>) {
        self.scan_dir_with_limit(dir, MAX_FILES, warnings);
    }

    #[cfg(test)]
    fn scan_dir_with_limit(&mut self, dir: &Path, limit: usize, warnings: &mut Vec<String>) {
        self.scan_dir_with_limit_and_prefix(dir, limit, None, warnings);
    }

    fn scan_dir_with_prefix(&mut self, dir: &Path, prefix: &str, warnings: &mut Vec<String>) {
        self.scan_dir_with_limit_and_prefix(dir, MAX_FILES, Some(prefix), warnings);
    }

    fn scan_dir_with_limit_and_prefix(
        &mut self,
        dir: &Path,
        limit: usize,
        prefix: Option<&str>,
        warnings: &mut Vec<String>,
    ) {
        if !dir.is_dir() {
            return;
        }

        let mut source_files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
            .map(|entry| entry.into_path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
            })
            .collect();
        source_files.sort();

        if source_files.len() > limit {
            warnings.push(format!(
                "检测到 {} 个 .cs 文件，超过安全上限 {limit}；仅索引前 {limit} 个，结果已截断。",
                source_files.len()
            ));
        }
        for path in source_files.iter().take(limit) {
            if let Some(prefix) = prefix {
                let relative = path.strip_prefix(dir).unwrap_or(path);
                let display = format!(
                    "{}{}",
                    prefix,
                    relative.to_string_lossy().replace('\\', "/")
                );
                self.scan_file_with_display(path, &display, warnings);
            } else {
                self.scan_file(path, warnings);
            }
        }
    }

    fn scan_file(&mut self, path: &Path, warnings: &mut Vec<String>) {
        self.scan_file_with_display(path, &path.display().to_string(), warnings);
    }

    fn scan_file_with_display(&mut self, path: &Path, display: &str, _warnings: &mut Vec<String>) {
        self.files_scanned += 1;
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let symbols = extract_types(&text, display);
        self.types.extend(symbols);
        self.source_files.push(SourceFile {
            path: display.into(),
            text,
        });
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
            .map(|t| {
                (
                    score_type(t, &asset_type_keys, &symbol_keys, item_name_key.as_deref()),
                    t,
                )
            })
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.full_name.cmp(&b.1.full_name))
        });
        scored.truncate(MAX_FACTS);

        scored
            .into_iter()
            .map(|(_, t)| symbol_to_fact(t, &asset_type_keys))
            .collect()
    }

    /// 从当前反编译源码按需求检索官方相似实现，再沿 override 名称查找生命周期调用方。
    /// 这里保存的是本次读取到的源码片段，不持久化行为结论。
    fn behavior_evidence_for_query(&self, query: &KnowledgeQuery) -> Vec<KnowledgeFactItem> {
        let Some(requirements) = query
            .requirements
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Vec::new();
        };
        let Some(asset_type) = query.asset_type.as_deref() else {
            return Vec::new();
        };
        let Some(path_marker) = asset_source_path_marker(asset_type) else {
            return Vec::new();
        };
        let intent = BehaviorSearchIntent::from_requirements(requirements);
        if intent.terms.len() < 2 {
            return Vec::new();
        }

        let similar = self
            .source_files
            .iter()
            .filter(|source| normalized_path(&source.path).contains(path_marker))
            .map(|source| (score_similar_source(source, &intent), source))
            .filter(|(score, _)| *score > 0)
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.path.cmp(&a.1.path)))
            .map(|(_, source)| source);
        let Some(similar) = similar else {
            return Vec::new();
        };

        let hook_name = relevant_override_method(&similar.text, &intent);
        let mut evidence = vec![source_evidence_fact(
            "official-similar-implementation",
            "Official similar implementation",
            similar,
            &intent.evidence_needles(),
            asset_type,
            -20,
        )];

        if let Some(hook_name) = hook_name {
            let hook_call = format!("Hook.{hook_name}");
            let lifecycle = self
                .source_files
                .iter()
                .filter(|source| source.path != similar.path && source.text.contains(&hook_call))
                .map(|source| (score_lifecycle_source(source, &intent, &hook_call), source))
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.path.cmp(&a.1.path)))
                .map(|(_, source)| source);
            if let Some(lifecycle) = lifecycle {
                let mut needles = vec![hook_call];
                if intent.energy {
                    needles.extend(["ResetEnergy".into(), "SetupPlayerTurn".into()]);
                }
                if intent.combat_start {
                    needles.push("BeforeCombatStart".into());
                }
                evidence.push(source_evidence_fact(
                    "lifecycle-caller",
                    "Lifecycle caller and surrounding state changes",
                    lifecycle,
                    &needles,
                    asset_type,
                    -10,
                ));
            }
        }
        evidence
    }
}

#[derive(Debug)]
struct BehaviorSearchIntent {
    terms: Vec<String>,
    energy: bool,
    gain: bool,
    combat_start: bool,
    numbers: Vec<String>,
}

impl BehaviorSearchIntent {
    fn from_requirements(requirements: &str) -> Self {
        let lower = requirements.to_ascii_lowercase();
        let energy = lower.contains("energy") || requirements.contains("能量");
        let gain = lower.contains("gain")
            || lower.contains("grant")
            || requirements.contains("获得")
            || requirements.contains("增加");
        let combat = lower.contains("combat") || requirements.contains("战斗");
        let start = lower.contains("start") || requirements.contains("开始");
        let mut terms: Vec<String> = re_ascii_word()
            .find_iter(&lower)
            .map(|value| value.as_str().to_string())
            .filter(|value| !is_requirement_stop_word(value))
            .collect();
        for (matched, term) in [
            (energy, "energy"),
            (gain, "gain"),
            (combat, "combat"),
            (start, "start"),
            (
                requirements.contains("回合") || lower.contains("turn"),
                "turn",
            ),
        ] {
            if matched && !terms.iter().any(|value| value == term) {
                terms.push(term.into());
            }
        }
        terms.sort();
        terms.dedup();
        let mut numbers: Vec<String> = re_number()
            .find_iter(requirements)
            .map(|value| value.as_str().to_string())
            .collect();
        numbers.sort();
        numbers.dedup();
        Self {
            terms,
            energy,
            gain,
            combat_start: combat && start,
            numbers,
        }
    }

    fn evidence_needles(&self) -> Vec<String> {
        let mut needles = Vec::new();
        if self.energy && self.gain {
            needles.push("GainEnergy".into());
        }
        if self.combat_start {
            needles.extend(["AfterSideTurnStart".into(), "RoundNumber".into()]);
        }
        needles.extend(self.terms.iter().cloned());
        needles
    }
}

fn re_ascii_word() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[a-z][a-z0-9_]{2,}").unwrap())
}

fn re_number() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap())
}

fn re_override_method() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(?:public|protected)\s+override\s+(?:async\s+)?[A-Za-z_][\w<>,\.\[\]\?\s]*?\s+([A-Za-z_]\w*)\s*\(",
        )
        .unwrap()
    })
}

fn is_requirement_stop_word(value: &str) -> bool {
    matches!(
        value,
        "the"
            | "and"
            | "for"
            | "with"
            | "this"
            | "that"
            | "from"
            | "into"
            | "only"
            | "each"
            | "every"
            | "new"
            | "relic"
            | "card"
            | "power"
            | "player"
    )
}

fn asset_source_path_marker(asset_type: &str) -> Option<&'static str> {
    match asset_type.trim().to_ascii_lowercase().as_str() {
        "relic" => Some("models/relics"),
        "card" | "card_fullscreen" => Some("models/cards"),
        "power" => Some("models/powers"),
        "character" => Some("models/characters"),
        _ => None,
    }
}

fn normalized_path(path: &str) -> String {
    path.replace('\\', "/")
        .replace('.', "/")
        .to_ascii_lowercase()
}

fn count_occurrences(haystack: &str, needle: &str) -> i32 {
    haystack.matches(needle).count().min(4) as i32
}

fn score_similar_source(source: &SourceFile, intent: &BehaviorSearchIntent) -> i32 {
    let lower = source.text.to_ascii_lowercase();
    let mut score = intent
        .terms
        .iter()
        .map(|term| count_occurrences(&lower, term))
        .sum::<i32>();
    if intent.energy && intent.gain && lower.contains(".gainenergy(") {
        score += 30;
    }
    if intent.combat_start && lower.contains("roundnumber") {
        score += 15;
        if lower.contains("roundnumber <= 1") || lower.contains("roundnumber == 1") {
            score += 10;
        }
    }
    if intent.energy {
        for number in &intent.numbers {
            let exact_var = format!("energyvar({number}");
            if lower.contains(&exact_var) {
                score += 25;
            }
        }
    }
    score
}

fn relevant_override_method(source: &str, intent: &BehaviorSearchIntent) -> Option<String> {
    let focus = if intent.energy && intent.gain {
        source.to_ascii_lowercase().find(".gainenergy(")
    } else {
        None
    };
    let candidates = re_override_method()
        .captures_iter(source)
        .filter_map(|capture| {
            let whole = capture.get(0)?;
            let name = capture.get(1)?.as_str().to_string();
            Some((whole.start(), name))
        });
    match focus {
        Some(focus) => candidates
            .filter(|(position, _)| *position < focus)
            .fold(None, |_, (_, name)| Some(name)),
        None => candidates.into_iter().next().map(|(_, name)| name),
    }
}

fn score_lifecycle_source(
    source: &SourceFile,
    intent: &BehaviorSearchIntent,
    hook_call: &str,
) -> i32 {
    let lower = source.text.to_ascii_lowercase();
    let mut score = count_occurrences(&lower, &hook_call.to_ascii_lowercase()) * 20;
    if intent.energy && lower.contains("resetenergy") {
        score += 30;
    }
    if lower.contains("setupplayerturn") {
        score += 20;
    }
    if intent.combat_start && lower.contains("beforecombatstart") {
        score += 10;
    }
    score
}

fn source_evidence_fact(
    key_suffix: &str,
    purpose: &str,
    source: &SourceFile,
    needles: &[String],
    asset_type: &str,
    priority: i32,
) -> KnowledgeFactItem {
    let (excerpt, ranges) = source_excerpt(&source.text, needles, 6);
    let range_text = ranges
        .iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect::<Vec<_>>()
        .join(", ");
    KnowledgeFactItem {
        key: format!("sts2.evidence.{key_suffix}"),
        title: purpose.into(),
        body: format!("Purpose: {purpose}\nSource lines: {range_text}\n```csharp\n{excerpt}\n```"),
        priority,
        evidence_paths: vec![source.path.clone()],
        keywords: needles.to_vec(),
        asset_types: vec![asset_type.into()],
    }
}

fn source_excerpt(text: &str, needles: &[String], context: usize) -> (String, Vec<(usize, usize)>) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (String::new(), Vec::new());
    }
    let lower_needles: Vec<String> = needles
        .iter()
        .filter(|needle| !needle.trim().is_empty())
        .map(|needle| needle.to_ascii_lowercase())
        .collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    'needles: for needle in &lower_needles {
        let mut matches_for_needle = 0;
        for (index, line) in lines.iter().enumerate() {
            if !line.to_ascii_lowercase().contains(needle) {
                continue;
            }
            ranges.push((
                index.saturating_sub(context),
                (index + context + 1).min(lines.len()),
            ));
            matches_for_needle += 1;
            if ranges.len() >= MAX_EVIDENCE_RANGES {
                break 'needles;
            }
            if matches_for_needle >= MAX_EVIDENCE_MATCHES_PER_NEEDLE {
                break;
            }
        }
    }
    if ranges.is_empty() {
        ranges.push((0, lines.len().min(context * 2 + 1)));
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
        } else {
            merged.push((start, end));
        }
    }
    let mut excerpt_parts = Vec::new();
    let mut one_based_ranges = Vec::new();
    for (start, end) in merged {
        one_based_ranges.push((start + 1, end));
        excerpt_parts.push(
            lines[start..end]
                .iter()
                .enumerate()
                .map(|(offset, line)| format!("{:>4}: {line}", start + offset + 1))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    (excerpt_parts.join("\n...\n"), one_based_ranges)
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
    if let Some(item_key) = item_name_key
        && name_lower.contains(item_key)
    {
        score += 3;
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
    use std::collections::BTreeMap;
    use std::fs;

    use sha2::{Digest, Sha256};

    use crate::game_pack::{
        GamePackLoadPolicy, GamePackLoader, LoadedGamePack, TruthSnapshotStore,
        VerifiedTruthSnapshot,
    };

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
        let (facts, warnings) =
            Sts2CodeFactsProvider.build_facts(&query, &paths, SourceMode::RuntimeDecompiled);

        assert_eq!(facts.len(), 1, "warnings={warnings:?}");
        let f = &facts[0];
        assert_eq!(f.key, "STS2.Cards.StrikeCard");
        assert!(f.title.contains("class STS2.Cards.StrikeCard"));
        assert!(
            f.body.contains("AbstractCard"),
            "body should mention base: {}",
            f.body
        );
        assert!(
            f.body.contains("Use"),
            "method excerpt missing in body: {}",
            f.body
        );
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
        let (facts, _w) =
            Sts2CodeFactsProvider.build_facts(&query, &paths, SourceMode::RuntimeDecompiled);

        assert!(
            facts.iter().any(|f| f.key.contains("RingOfFireRelic")),
            "should match Relic by base class; got: {:?}",
            facts.iter().map(|f| &f.key).collect::<Vec<_>>()
        );
        // HelperUtility 无 asset_type 关键字命中，不应进入结果
        assert!(facts.iter().all(|f| !f.key.contains("HelperUtility")));
    }

    #[test]
    fn behavior_requirement_injects_official_implementation_and_lifecycle_caller() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();
        write_cs(
            &paths.game_dir,
            "MegaCrit.Sts2.Core.Models.Relics/Bread.cs",
            r#"namespace MegaCrit.Sts2.Core.Models.Relics;
public sealed class Bread : RelicModel
{
    protected override IEnumerable<DynamicVar> CanonicalVars => new[] { new EnergyVar(1) };
    public override Task BeforeCombatStart() => Task.CompletedTask;
}"#,
        );
        write_cs(
            &paths.game_dir,
            "MegaCrit.Sts2.Core.Models.Relics/Lantern.cs",
            r#"namespace MegaCrit.Sts2.Core.Models.Relics;
public sealed class Lantern : RelicModel
{
    protected override IEnumerable<DynamicVar> CanonicalVars => new[] { new EnergyVar(1) };
    public override async Task AfterSideTurnStart(CombatSide side, CombatState combatState)
    {
        if (side == base.Owner.Creature.Side && combatState.RoundNumber <= 1)
            await PlayerCmd.GainEnergy(base.DynamicVars.Energy.BaseValue, base.Owner);
    }
}"#,
        );
        write_cs(
            &paths.game_dir,
            "MegaCrit.Sts2.Core.Combat/CombatManager.cs",
            r#"namespace MegaCrit.Sts2.Core.Combat;
public sealed class CombatManager
{
    public async Task StartCombatInternal() { await Hook.BeforeCombatStart(_state.RunState, _state); }
    public async Task StartSideTurn()
    {
        await SetupPlayerTurn(player);
        await Hook.AfterSideTurnStart(_state, _state.CurrentSide);
    }
    private async Task SetupPlayerTurn(Player player)
    {
        player.PlayerCombatState.ResetEnergy();
    }
}"#,
        );

        let query = KnowledgeQuery {
            asset_type: Some("relic".into()),
            requirements: Some(
                "At the start of combat, gain 1 Energy. 战斗开始时获得1点能量。".into(),
            ),
            ..Default::default()
        };
        let (facts, warnings) =
            Sts2CodeFactsProvider.build_facts(&query, &paths, SourceMode::RuntimeDecompiled);

        let similar = facts
            .iter()
            .find(|fact| fact.key == "sts2.evidence.official-similar-implementation")
            .unwrap_or_else(|| panic!("missing similar evidence; warnings={warnings:?}"));
        assert!(similar.body.contains("AfterSideTurnStart"));
        assert!(similar.body.contains("RoundNumber <= 1"));
        assert!(similar.evidence_paths[0].ends_with("Lantern.cs"));

        let lifecycle = facts
            .iter()
            .find(|fact| fact.key == "sts2.evidence.lifecycle-caller")
            .expect("lifecycle caller evidence");
        assert!(lifecycle.body.contains("Hook.AfterSideTurnStart"));
        assert!(lifecycle.body.contains("ResetEnergy"));
        assert!(lifecycle.evidence_paths[0].ends_with("CombatManager.cs"));
    }

    #[test]
    fn source_evidence_excerpt_is_bounded_and_keeps_high_priority_needles() {
        let mut lines = (1..=500)
            .map(|line| format!("// generic combat marker {line}"))
            .collect::<Vec<_>>();
        lines[399] = "await Hook.AfterSideTurnStart(_state, side);".into();
        lines[449] = "player.PlayerCombatState.ResetEnergy();".into();
        let source = lines.join("\n");
        let needles = vec![
            "Hook.AfterSideTurnStart".into(),
            "ResetEnergy".into(),
            "combat".into(),
        ];

        let (excerpt, ranges) = source_excerpt(&source, &needles, 2);

        assert!(excerpt.contains("Hook.AfterSideTurnStart"));
        assert!(excerpt.contains("ResetEnergy"));
        assert!(ranges.len() <= MAX_EVIDENCE_RANGES);
        assert!(excerpt.lines().count() <= MAX_EVIDENCE_RANGES * 5 + MAX_EVIDENCE_RANGES - 1);
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
        let (facts, _w) =
            Sts2CodeFactsProvider.build_facts(&query, &paths, SourceMode::RuntimeDecompiled);
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
        let (facts, _w) =
            Sts2CodeFactsProvider.build_facts(&query, &paths, SourceMode::RuntimeDecompiled);
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
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("BaseLib.decompiled.cs 不存在"))
        );
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
                &format!("namespace STS2.Cards {{ public class Card{i} : AbstractCard {{ }} }}"),
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

    #[test]
    fn scans_more_than_the_legacy_two_thousand_file_limit() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        crate::knowledge::ensure_dirs(&paths).unwrap();

        for i in 0..2_001 {
            write_cs(
                &paths.game_dir,
                &format!("Generated{i:04}.cs"),
                &format!("namespace STS2.Generated {{ public class Generated{i:04} {{ }} }}"),
            );
        }
        write_cs(
            &paths.game_dir,
            "zzzz/NSettingsScreen.cs",
            "namespace STS2.Screens { public class NSettingsScreen { } }",
        );

        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery {
                symbols: vec!["NSettingsScreen".into()],
                ..Default::default()
            },
            &paths,
            SourceMode::RuntimeDecompiled,
        );

        assert!(
            facts
                .iter()
                .any(|fact| fact.key == "STS2.Screens.NSettingsScreen"),
            "the complete game source should be indexed; warnings={warnings:?}"
        );
        assert!(
            warnings.iter().all(|warning| !warning.contains("文件上限")),
            "the normal STS2 source size must not be reported as truncated: {warnings:?}"
        );
    }

    #[test]
    fn scan_dir_is_deterministic_and_reports_explicit_truncation() {
        let td = tempfile::TempDir::new().unwrap();
        for rel in ["z/Z.cs", "a/A.cs", "m/M.cs"] {
            write_cs(
                td.path(),
                rel,
                "namespace STS2 { public class Example { } }",
            );
        }

        let mut first = CodeFactsIndex::default();
        let mut first_warnings = Vec::new();
        first.scan_dir_with_limit(td.path(), 2, &mut first_warnings);

        let mut second = CodeFactsIndex::default();
        let mut second_warnings = Vec::new();
        second.scan_dir_with_limit(td.path(), 2, &mut second_warnings);

        let first_paths: Vec<_> = first.types.iter().map(|symbol| &symbol.file_path).collect();
        let second_paths: Vec<_> = second
            .types
            .iter()
            .map(|symbol| &symbol.file_path)
            .collect();
        assert_eq!(first_paths, second_paths);
        assert!(first_paths[0].ends_with("a\\A.cs") || first_paths[0].ends_with("a/A.cs"));
        assert!(first_paths[1].ends_with("m\\M.cs") || first_paths[1].ends_with("m/M.cs"));
        assert_eq!(first.files_scanned, 2);
        assert_eq!(first_warnings, second_warnings);
        assert!(
            first_warnings.iter().any(|warning| {
                warning.contains("3") && warning.contains("2") && warning.contains("截断")
            }),
            "warnings={first_warnings:?}"
        );
    }

    const SNAPSHOT_PROVIDER: &str = "sts2_code_facts";
    const SNAPSHOT_GAME_BYTES: &[u8] = b"fixture-current-game";
    const SNAPSHOT_BASELIB_BYTES: &[u8] = b"fixture-pinned-baselib";

    fn equivalence_pack() -> LoadedGamePack {
        let baselib_sha = format!("{:x}", Sha256::digest(SNAPSHOT_BASELIB_BYTES));
        let json = format!(
            r#"{{
              "schema_version": 1,
              "id": "equivalence-game",
              "display_name": "Equivalence Game",
              "capabilities": ["truth_sources"],
              "truth_sources": [
                {{
                  "id": "game",
                  "kind": "local_file",
                  "input_key": "game_assembly",
                  "indexer": "dotnet_project",
                  "provider": "{SNAPSHOT_PROVIDER}"
                }},
                {{
                  "id": "baselib",
                  "kind": "github_release_asset",
                  "repository": "owner/repository",
                  "pinned_release": "v1.2.3",
                  "asset": "BaseLib.dll",
                  "sha256": "{baselib_sha}",
                  "indexer": "dotnet_file",
                  "provider": "{SNAPSHOT_PROVIDER}"
                }}
              ]
            }}"#
        );
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            [SNAPSHOT_PROVIDER],
        ))
        .load_str("equivalence-pack", &json)
        .unwrap()
    }

    fn write_equivalence_game_sources(root: &Path) {
        write_cs(
            root,
            "MegaCrit.Sts2.Core.Models.Relics/Lantern.cs",
            r#"namespace MegaCrit.Sts2.Core.Models.Relics;
public sealed class Lantern : RelicModel
{
    protected override IEnumerable<DynamicVar> CanonicalVars => new[] { new EnergyVar(1) };
    public override async Task AfterSideTurnStart(CombatSide side, CombatState combatState)
    {
        if (side == base.Owner.Creature.Side && combatState.RoundNumber <= 1)
            await PlayerCmd.GainEnergy(base.DynamicVars.Energy.BaseValue, base.Owner);
    }
}"#,
        );
        write_cs(
            root,
            "MegaCrit.Sts2.Core.Combat/CombatManager.cs",
            r#"namespace MegaCrit.Sts2.Core.Combat;
public sealed class CombatManager
{
    public async Task StartSideTurn()
    {
        await SetupPlayerTurn(player);
        await Hook.AfterSideTurnStart(_state, _state.CurrentSide);
    }
    private async Task SetupPlayerTurn(Player player)
    {
        player.PlayerCombatState.ResetEnergy();
    }
}"#,
        );
    }

    fn write_equivalence_baselib_source(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            r#"namespace BaseLib;
public class BaseLibRelicSupport : RelicModel
{
    public void RegisterRelic() { }
}"#,
        )
        .unwrap();
    }

    fn equivalence_snapshot(
        temp: &tempfile::TempDir,
        pack: &LoadedGamePack,
    ) -> VerifiedTruthSnapshot {
        let raw_root = temp.path().join("raw-sources");
        fs::create_dir_all(&raw_root).unwrap();
        let game_dll = raw_root.join("game.dll");
        let baselib_dll = raw_root.join("BaseLib.dll");
        fs::write(&game_dll, SNAPSHOT_GAME_BYTES).unwrap();
        fs::write(&baselib_dll, SNAPSHOT_BASELIB_BYTES).unwrap();

        let store = TruthSnapshotStore::new(&temp.path().join("snapshot-runtime"), pack);
        let mut draft = store.begin(pack).unwrap();
        draft.stage_source("game", &game_dll).unwrap();
        draft.stage_source("baselib", &baselib_dll).unwrap();
        write_equivalence_game_sources(&draft.index_output_dir("game").unwrap());
        write_equivalence_baselib_source(
            &draft
                .index_output_dir("baselib")
                .unwrap()
                .join("BaseLib.decompiled.cs"),
        );
        draft
            .finalize(BTreeMap::from([("fixture-indexer".into(), "1.0.0".into())]))
            .unwrap()
    }

    fn normalized_facts(
        facts: &[KnowledgeFactItem],
        legacy: &KnowledgePaths,
        snapshot: &VerifiedTruthSnapshot,
    ) -> serde_json::Value {
        let legacy_game = legacy.game_dir.to_string_lossy().replace('\\', "/");
        let legacy_baselib = legacy.baselib_dir.to_string_lossy().replace('\\', "/");
        let snapshot_prefix = format!("snapshot://{}/", snapshot.snapshot_id());
        let normalize = |value: &str| {
            value
                .replace('\\', "/")
                .replace(&legacy_game, "game")
                .replace(&legacy_baselib, "baselib")
                .replace(&snapshot_prefix, "")
        };
        let mut normalized = facts.to_vec();
        for fact in &mut normalized {
            fact.body = normalize(&fact.body);
            fact.evidence_paths = fact
                .evidence_paths
                .iter()
                .map(|path| normalize(path))
                .collect();
        }
        serde_json::to_value(normalized).unwrap()
    }

    #[test]
    fn legacy_and_snapshot_fact_selection_are_semantically_equivalent() {
        let temp = tempfile::TempDir::new().unwrap();
        let legacy = KnowledgePaths::from_runtime_dir(&temp.path().join("legacy-runtime"));
        crate::knowledge::ensure_dirs(&legacy).unwrap();
        write_equivalence_game_sources(&legacy.game_dir);
        write_equivalence_baselib_source(&legacy.baselib_decompiled_file());

        let pack = equivalence_pack();
        let snapshot = equivalence_snapshot(&temp, &pack);
        let provider = Sts2CodeFactsProvider;
        let queries = [
            KnowledgeQuery {
                asset_type: Some("relic".into()),
                requirements: Some(
                    "At the start of combat, gain 1 Energy. 战斗开始时获得1点能量。".into(),
                ),
                ..Default::default()
            },
            KnowledgeQuery {
                symbols: vec!["BaseLibRelicSupport".into()],
                ..Default::default()
            },
            KnowledgeQuery {
                asset_type: Some("relic".into()),
                ..Default::default()
            },
        ];

        for query in queries {
            let (legacy_facts, legacy_warnings) =
                provider.build_facts(&query, &legacy, SourceMode::RuntimeDecompiled);
            let (snapshot_facts, snapshot_warnings) = provider
                .build_facts_from_snapshot(&query, &snapshot, SNAPSHOT_PROVIDER)
                .unwrap();
            assert_eq!(legacy_warnings, snapshot_warnings);
            assert_eq!(
                normalized_facts(&legacy_facts, &legacy, &snapshot),
                normalized_facts(&snapshot_facts, &legacy, &snapshot),
                "query={query:?}"
            );
        }
    }

    #[test]
    fn snapshot_fact_provider_rejects_an_unmapped_provider() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = equivalence_pack();
        let snapshot = equivalence_snapshot(&temp, &pack);
        let error = Sts2CodeFactsProvider
            .build_facts_from_snapshot(&KnowledgeQuery::default(), &snapshot, "missing-provider")
            .unwrap_err();
        assert!(matches!(
            error,
            SnapshotCodeFactsError::MissingProvider { provider, .. }
                if provider == "missing-provider"
        ));
    }
}
