//! Prompt 模板加载 + 变量替换。镜像 Python `PromptLoader`。
//!
//! 与 Python 端的差异：
//! - **不读磁盘**——所有模板通过 `include_str!` 内嵌；运行期不再有 IO。
//!   测试可用 [`PromptLoader::with_files`] 注入任意 `name → content` 集合。
//! - bundle 解析（`## section_key`）逻辑完全保留，包括代码 fence 保护、空段
//!   报错、重复 key 报错。
//!
//! 模板语法：`{{ name }}` 占位符；缺变量时返回错误（与 Python 行为一致）。

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::{Mutex, MutexGuard};

use regex::Regex;
use thiserror::Error;

/// Built-in prompt registry — 6 个模板内嵌进二进制。
fn built_in_files() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert(
        "analyzer.md".into(),
        include_str!("../../prompts/analyzer.md").into(),
    );
    m.insert(
        "codegen.md".into(),
        include_str!("../../prompts/codegen.md").into(),
    );
    m.insert(
        "image.md".into(),
        include_str!("../../prompts/image.md").into(),
    );
    m.insert(
        "runtime_agent.md".into(),
        include_str!("../../prompts/runtime_agent.md").into(),
    );
    m.insert(
        "runtime_system.md".into(),
        include_str!("../../prompts/runtime_system.md").into(),
    );
    m.insert(
        "runtime_workflow.md".into(),
        include_str!("../../prompts/runtime_workflow.md").into(),
    );
    m
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("prompt template not found: {0}")]
    NotFound(String),
    #[error("missing template variable: {0}")]
    MissingVariable(String),
    #[error("invalid prompt bundle key: {0}")]
    InvalidBundleKey(String),
    #[error("duplicate prompt bundle key: {0}")]
    DuplicateBundleKey(String),
    #[error("empty prompt bundle section: {0}")]
    EmptyBundleSection(String),
}

type BundleSections = HashMap<String, String>;

pub struct PromptLoader {
    files: HashMap<String, String>,
    bundle_cache: Mutex<HashMap<String, Result<BundleSections, PromptError>>>,
}

impl PromptLoader {
    /// 用内置 6 个 prompt（analyzer / codegen / image / runtime_agent /
    /// runtime_system / runtime_workflow）。
    #[must_use]
    pub fn built_in() -> Self {
        Self::with_files(built_in_files())
    }

    /// 用任意 `file_name → content` 集合（主要给测试用）。
    #[must_use]
    pub fn with_files(files: HashMap<String, String>) -> Self {
        Self {
            files,
            bundle_cache: Mutex::new(HashMap::new()),
        }
    }

    /// 装载模板内容。`template_name` 形式：
    /// - `"foo.md"`：直接返回文件内容
    /// - `"foo.bar"`：把 `foo.md` 解析成 `## section_key` 分段，返回 `bar` 段
    pub fn load(&self, template_name: &str) -> Result<String, PromptError> {
        if let Some((bundle_name, key)) = parse_bundle_request(template_name) {
            let bundle_file = format!("{bundle_name}.md");
            if !self.files.contains_key(&bundle_file) {
                return Err(PromptError::NotFound(template_name.into()));
            }
            // 从缓存或解析中获取分段
            let sections = self.bundle_sections(&bundle_name)?;
            return sections
                .get(&key)
                .cloned()
                .ok_or_else(|| PromptError::NotFound(template_name.into()));
        }

        self.files
            .get(template_name)
            .cloned()
            .ok_or_else(|| PromptError::NotFound(template_name.into()))
    }

    /// 装载 + 变量替换。`vars` 中缺失的占位符会 `Err(MissingVariable)`。
    pub fn render(
        &self,
        template_name: &str,
        vars: &HashMap<&str, &str>,
    ) -> Result<String, PromptError> {
        let template = self.load(template_name)?;
        render_template_string(&template, vars)
    }

    fn bundle_sections(&self, bundle_name: &str) -> Result<BundleSections, PromptError> {
        // 先看 cache。若缓存的是 Err，克隆该 Err 返回。
        let mut cache = self.lock_cache();
        if let Some(cached) = cache.get(bundle_name) {
            return cached.as_ref().map(Clone::clone).map_err(clone_prompt_err);
        }

        let bundle_file = format!("{bundle_name}.md");
        let Some(content) = self.files.get(&bundle_file) else {
            let err = PromptError::NotFound(bundle_name.into());
            cache.insert(bundle_name.into(), Err(clone_prompt_err(&err)));
            return Err(err);
        };

        let parsed = parse_bundle_sections(content);
        let to_store: Result<BundleSections, PromptError> = match &parsed {
            Ok(s) => Ok(s.clone()),
            Err(e) => Err(clone_prompt_err(e)),
        };
        cache.insert(bundle_name.into(), to_store);
        parsed
    }

    fn lock_cache(&self) -> MutexGuard<'_, HashMap<String, Result<BundleSections, PromptError>>> {
        self.bundle_cache
            .lock()
            .expect("prompt loader bundle cache poisoned")
    }
}

impl Default for PromptLoader {
    fn default() -> Self {
        Self::built_in()
    }
}

fn template_var_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap())
}

fn bundle_key_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z0-9_]+$").unwrap())
}

fn fence_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*(`{3,}|~{3,})").unwrap())
}

const BUNDLE_HEADING_PREFIX: &str = "## ";

/// 解析 `"bundle.key"` 形式。失败时返回 None，调用方应当成直接文件名处理。
fn parse_bundle_request(template_name: &str) -> Option<(String, String)> {
    if template_name.contains('/') || template_name.contains('\\') {
        return None;
    }
    if template_name.ends_with(".md") {
        return None;
    }
    let parts: Vec<&str> = template_name.split('.').collect();
    if parts.len() != 2 {
        return None;
    }
    let (bundle_name, key) = (parts[0], parts[1]);
    if bundle_name.is_empty() || key.is_empty() {
        return None;
    }
    if !bundle_key_pattern().is_match(bundle_name) || !bundle_key_pattern().is_match(key) {
        return None;
    }
    Some((bundle_name.into(), key.into()))
}

/// 将 bundle 内容按 `## key` 切段。代码 fence 内的 `##` 不算 heading。
fn parse_bundle_sections(content: &str) -> Result<BundleSections, PromptError> {
    let mut sections: BundleSections = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut in_fence = false;
    let mut active_fence: Option<char> = None;

    for line in content.split_inclusive('\n') {
        if let Some(caps) = fence_pattern().captures(line) {
            let fence = caps.get(1).unwrap().as_str();
            let fence_char = fence.chars().next().unwrap();
            if in_fence && active_fence == Some(fence_char) {
                in_fence = false;
                active_fence = None;
            } else if !in_fence {
                in_fence = true;
                active_fence = Some(fence_char);
            }
        }

        let heading_key = if in_fence { None } else { parse_heading(line)? };
        if heading_key.is_none() {
            if current_key.is_some() {
                current_lines.push(line.to_string());
            }
            continue;
        }
        let heading_key = heading_key.unwrap();

        if Some(&heading_key) == current_key.as_ref() || sections.contains_key(&heading_key) {
            return Err(PromptError::DuplicateBundleKey(heading_key));
        }

        if let Some(prev_key) = current_key.take() {
            let body = finalize_section(&prev_key, &current_lines)?;
            sections.insert(prev_key, body);
            current_lines.clear();
        }
        current_key = Some(heading_key);
    }

    if let Some(prev_key) = current_key {
        let body = finalize_section(&prev_key, &current_lines)?;
        sections.insert(prev_key, body);
    }

    Ok(sections)
}

fn parse_heading(line: &str) -> Result<Option<String>, PromptError> {
    if !line.starts_with(BUNDLE_HEADING_PREFIX) {
        return Ok(None);
    }
    let key = line[BUNDLE_HEADING_PREFIX.len()..].trim();
    if !bundle_key_pattern().is_match(key) {
        return Err(PromptError::InvalidBundleKey(key.into()));
    }
    Ok(Some(key.into()))
}

fn finalize_section(key: &str, lines: &[String]) -> Result<String, PromptError> {
    let content: String = lines.concat();
    if content.trim().is_empty() {
        return Err(PromptError::EmptyBundleSection(key.into()));
    }
    Ok(content)
}

fn render_template_string(
    template: &str,
    vars: &HashMap<&str, &str>,
) -> Result<String, PromptError> {
    let re = template_var_pattern();
    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;
    for caps in re.captures_iter(template) {
        let full = caps.get(0).unwrap();
        let key = caps.get(1).unwrap().as_str();
        result.push_str(&template[last_end..full.start()]);
        match vars.get(key) {
            Some(value) => result.push_str(value),
            None => return Err(PromptError::MissingVariable(key.into())),
        }
        last_end = full.end();
    }
    result.push_str(&template[last_end..]);
    Ok(result)
}

fn clone_prompt_err(err: &PromptError) -> PromptError {
    match err {
        PromptError::NotFound(s) => PromptError::NotFound(s.clone()),
        PromptError::MissingVariable(s) => PromptError::MissingVariable(s.clone()),
        PromptError::InvalidBundleKey(s) => PromptError::InvalidBundleKey(s.clone()),
        PromptError::DuplicateBundleKey(s) => PromptError::DuplicateBundleKey(s.clone()),
        PromptError::EmptyBundleSection(s) => PromptError::EmptyBundleSection(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader_with(file_name: &str, content: &str) -> PromptLoader {
        let mut m = HashMap::new();
        m.insert(file_name.into(), content.into());
        PromptLoader::with_files(m)
    }

    #[test]
    fn render_substitutes_variables() {
        let loader = loader_with("hello.md", "Hi {{ name }}, welcome to {{ place }}.");
        let mut vars = HashMap::new();
        vars.insert("name", "Alice");
        vars.insert("place", "Spire");
        let out = loader.render("hello.md", &vars).unwrap();
        assert_eq!(out, "Hi Alice, welcome to Spire.");
    }

    #[test]
    fn render_missing_variable_errors() {
        let loader = loader_with("hello.md", "{{ ghost }}");
        let err = loader.render("hello.md", &HashMap::new()).unwrap_err();
        assert!(matches!(err, PromptError::MissingVariable(s) if s == "ghost"));
    }

    #[test]
    fn bundle_section_lookup() {
        let bundle = "preamble\n\n## one\nFirst section.\n\n## two\nSecond section.\n";
        let loader = loader_with("kit.md", bundle);
        assert_eq!(loader.load("kit.one").unwrap().trim(), "First section.");
        assert_eq!(loader.load("kit.two").unwrap().trim(), "Second section.");
    }

    #[test]
    fn fence_inside_section_does_not_split() {
        let bundle = "## one\nbody\n```\n## not_a_heading\n```\nmore body\n## two\nB\n";
        let loader = loader_with("kit.md", bundle);
        let one = loader.load("kit.one").unwrap();
        assert!(one.contains("## not_a_heading"));
        assert!(one.contains("more body"));
        assert!(!one.contains("## two"));
    }

    #[test]
    fn duplicate_bundle_key_errors() {
        let bundle = "## one\nA\n## one\nA again\n";
        let loader = loader_with("kit.md", bundle);
        let err = loader.load("kit.one").unwrap_err();
        assert!(matches!(err, PromptError::DuplicateBundleKey(_)));
    }

    #[test]
    fn missing_template_errors() {
        let loader = PromptLoader::with_files(HashMap::new());
        let err = loader.load("ghost.md").unwrap_err();
        assert!(matches!(err, PromptError::NotFound(_)));
    }

    #[test]
    fn built_in_loader_serves_codegen() {
        let loader = PromptLoader::built_in();
        let content = loader.load("codegen.md").unwrap();
        assert!(!content.is_empty(), "built-in codegen.md should not be empty");
    }
}
