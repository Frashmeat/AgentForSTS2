//! 把 `KnowledgePacket` 渲染成 5 段 Markdown 字符串（facts / guidance / lookup /
//! knowledge_warnings / summary）。镜像 Python `PromptContextAssembler.assemble`。

use std::collections::HashMap;

use crate::knowledge::{
    KnowledgeFactItem, KnowledgeGuidanceItem, KnowledgeLookupItem, KnowledgePacket,
};

#[derive(Debug, Default, Clone)]
pub struct PromptContextAssembler;

impl PromptContextAssembler {
    pub fn assemble(&self, packet: &KnowledgePacket) -> HashMap<String, String> {
        let mut out = HashMap::with_capacity(5);
        out.insert("facts".into(), render_facts(&packet.facts));
        out.insert("guidance".into(), render_guidance(&packet.guidance));
        out.insert("lookup".into(), render_lookup(&packet.lookup));
        out.insert("knowledge_warnings".into(), render_warnings(&packet.warnings));
        out.insert("summary".into(), packet.summary.trim().to_string());
        out
    }
}

fn render_facts(facts: &[KnowledgeFactItem]) -> String {
    if facts.is_empty() {
        return String::new();
    }
    let mut ordered: Vec<&KnowledgeFactItem> = facts.iter().collect();
    ordered.sort_by(|a, b| (a.priority, a.key.as_str()).cmp(&(b.priority, b.key.as_str())));
    ordered
        .iter()
        .map(|item| render_fact_item(item))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_guidance(guidance: &[KnowledgeGuidanceItem]) -> String {
    if guidance.is_empty() {
        return String::new();
    }
    let mut ordered: Vec<&KnowledgeGuidanceItem> = guidance.iter().collect();
    ordered.sort_by(|a, b| a.key.cmp(&b.key));
    ordered
        .iter()
        .map(|item| render_guidance_item(item))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_lookup(lookup: &[KnowledgeLookupItem]) -> String {
    if lookup.is_empty() {
        return String::new();
    }
    let mut ordered: Vec<&KnowledgeLookupItem> = lookup.iter().collect();
    ordered.sort_by(|a, b| a.key.cmp(&b.key));
    ordered
        .iter()
        .map(|item| render_lookup_item(item))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_warnings(warnings: &[String]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let mut lines = vec!["### Warnings".to_string()];
    lines.extend(warnings.iter().map(|w| format!("- {w}")));
    lines.join("\n")
}

fn render_fact_item(item: &KnowledgeFactItem) -> String {
    let mut lines = vec![format!("### {}", item.title), item.body.trim().to_string()];
    if !item.evidence_paths.is_empty() {
        lines.push("Evidence paths:".into());
        for path in &item.evidence_paths {
            lines.push(format!("- `{path}`"));
        }
    }
    lines.join("\n").trim().to_string()
}

fn render_guidance_item(item: &KnowledgeGuidanceItem) -> String {
    let mut lines = vec![format!("### {}", item.title), item.body.trim().to_string()];
    if !item.source_path.is_empty() {
        lines.push(format!("Source: `{}`", item.source_path));
    }
    lines.join("\n").trim().to_string()
}

fn render_lookup_item(item: &KnowledgeLookupItem) -> String {
    let mut lines = vec![format!("### {}", item.title), format!("Path: `{}`", item.path)];
    if !item.note.is_empty() {
        lines.push(item.note.trim().to_string());
    }
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::KnowledgePacket;

    #[test]
    fn empty_packet_yields_empty_strings() {
        let packet = KnowledgePacket::default();
        let out = PromptContextAssembler.assemble(&packet);
        assert_eq!(out["facts"], "");
        assert_eq!(out["guidance"], "");
        assert_eq!(out["lookup"], "");
        assert_eq!(out["knowledge_warnings"], "");
        assert_eq!(out["summary"], "");
    }

    #[test]
    fn facts_sorted_by_priority_then_key() {
        let packet = KnowledgePacket {
            facts: vec![
                KnowledgeFactItem {
                    key: "z".into(),
                    title: "Low priority Z".into(),
                    body: "body-z".into(),
                    priority: 10,
                    ..Default::default()
                },
                KnowledgeFactItem {
                    key: "a".into(),
                    title: "High priority A".into(),
                    body: "body-a".into(),
                    priority: 1,
                    ..Default::default()
                },
            ],
            ..KnowledgePacket::default()
        };
        let out = PromptContextAssembler.assemble(&packet);
        let priority_a = out["facts"].find("High priority A").unwrap();
        let priority_z = out["facts"].find("Low priority Z").unwrap();
        assert!(priority_a < priority_z);
    }

    #[test]
    fn fact_with_evidence_paths_renders_backticked() {
        let packet = KnowledgePacket {
            facts: vec![KnowledgeFactItem {
                key: "f".into(),
                title: "T".into(),
                body: "Body".into(),
                priority: 1,
                evidence_paths: vec!["src/foo.rs".into(), "src/bar.rs".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = PromptContextAssembler.assemble(&packet);
        assert!(out["facts"].contains("- `src/foo.rs`"));
        assert!(out["facts"].contains("- `src/bar.rs`"));
    }

    #[test]
    fn warnings_rendered_as_bullet_list() {
        let packet = KnowledgePacket {
            warnings: vec!["first".into(), "second".into()],
            ..Default::default()
        };
        let out = PromptContextAssembler.assemble(&packet);
        assert_eq!(out["knowledge_warnings"], "### Warnings\n- first\n- second");
    }
}
