use regex::Regex;

use crate::game_pack::ValidationRule;

pub(crate) fn validate_localization_rich_text(
    value: &str,
    allowed_tags: &[String],
) -> Result<(), String> {
    let allowed = allowed_tags
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let mut open_tags = Vec::new();
    let bytes = value.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'[' => {
                let Some(relative_end) = bytes[cursor + 1..].iter().position(|byte| *byte == b']')
                else {
                    return Err("localization rich-text tag is missing a closing `]`".into());
                };
                let end = cursor + 1 + relative_end;
                let token = &value[cursor + 1..end];
                let (is_closing, tag) = token
                    .strip_prefix('/')
                    .map_or((false, token), |tag| (true, tag));
                if tag.is_empty()
                    || !tag.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '_'
                            || character == '-'
                    })
                    || !allowed.contains(tag)
                {
                    return Err(format!(
                        "localization rich-text tag `{token}` is not allowed; allowed tags: {}",
                        allowed.iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
                if is_closing {
                    let Some(open) = open_tags.pop() else {
                        return Err(format!(
                            "localization rich-text closing tag `/{tag}` has no matching opening tag"
                        ));
                    };
                    if open != tag {
                        return Err(format!(
                            "localization rich-text closing tag `/{tag}` does not match open tag `{open}`"
                        ));
                    }
                } else {
                    open_tags.push(tag);
                }
                cursor = end + 1;
            }
            b']' => {
                return Err(
                    "localization contains `]` without a matching rich-text opening tag".into(),
                );
            }
            _ => cursor += 1,
        }
    }

    if let Some(tag) = open_tags.last() {
        return Err(format!(
            "localization rich-text opening tag `{tag}` is not closed"
        ));
    }
    Ok(())
}

pub(crate) fn validate_generated_csharp(
    source: &str,
    rules: &[ValidationRule],
) -> Result<(), String> {
    let masked = mask_csharp_comments_and_literals(source);
    for rule in rules {
        let ValidationRule::ForbiddenCallInMethod {
            id,
            method_name,
            call_path,
            message,
        } = rule;
        let method_pattern = Regex::new(&format!(
            r"(?m)(?:^|[{{}};])\s*(?:public|protected|internal|private)\s+(?:(?:static|virtual|override|async|sealed|new)\s+)*[A-Za-z_][\w<>,\.\[\]\?]*\s+{}\s*\(",
            regex::escape(method_name)
        ))
        .expect("validated method rule must compile");
        let call_pattern = Regex::new(&format!(
            r"\b{}\s*\(",
            call_path
                .iter()
                .map(|part| regex::escape(part))
                .collect::<Vec<_>>()
                .join(r"\s*\.\s*")
        ))
        .expect("validated call rule must compile");

        for method_match in method_pattern.find_iter(&masked) {
            let open_paren = method_match.end() - 1;
            let Some(close_paren) = matching_delimiter(&masked, open_paren, b'(', b')') else {
                continue;
            };
            let Some(body) = method_body(&masked, close_paren + 1) else {
                continue;
            };
            if call_pattern.is_match(body) {
                return Err(format!(
                    "semantic rule {} rejected generated C#: {}",
                    id, message
                ));
            }
        }
    }
    Ok(())
}

fn method_body(source: &str, mut cursor: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => {
                let end = matching_delimiter(source, cursor, b'{', b'}')?;
                return source.get(cursor + 1..end);
            }
            b'=' if bytes.get(cursor + 1) == Some(&b'>') => {
                let start = cursor + 2;
                let end = bytes[start..]
                    .iter()
                    .position(|byte| *byte == b';')
                    .map(|offset| start + offset)?;
                return source.get(start..end);
            }
            b';' => return None,
            _ => cursor += 1,
        }
    }
    None
}

fn matching_delimiter(source: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&open) {
        return None;
    }
    let mut depth = 0_u32;
    for (offset, byte) in bytes[start..].iter().copied().enumerate() {
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(start + offset);
            }
        }
    }
    None
}

fn mask_csharp_comments_and_literals(source: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        String { quote: u8, verbatim: bool },
    }

    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut state = State::Code;
    let mut index = 0;
    while index < bytes.len() {
        match state {
            State::Code => {
                if bytes[index..].starts_with(b"//") {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                    state = State::LineComment;
                } else if bytes[index..].starts_with(b"/*") {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                    state = State::BlockComment;
                } else if bytes[index..].starts_with(b"$@\"") || bytes[index..].starts_with(b"@$\"")
                {
                    for value in &mut masked[index..index + 3] {
                        *value = b' ';
                    }
                    index += 3;
                    state = State::String {
                        quote: b'"',
                        verbatim: true,
                    };
                } else if bytes[index..].starts_with(b"@\"") {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                    state = State::String {
                        quote: b'"',
                        verbatim: true,
                    };
                } else if bytes[index..].starts_with(b"$\"") {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                    state = State::String {
                        quote: b'"',
                        verbatim: false,
                    };
                } else if matches!(bytes[index], b'"' | b'\'') {
                    let quote = bytes[index];
                    masked[index] = b' ';
                    index += 1;
                    state = State::String {
                        quote,
                        verbatim: false,
                    };
                } else {
                    index += 1;
                }
            }
            State::LineComment => {
                if bytes[index] == b'\n' {
                    state = State::Code;
                } else {
                    masked[index] = b' ';
                }
                index += 1;
            }
            State::BlockComment => {
                if bytes[index..].starts_with(b"*/") {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                    state = State::Code;
                } else {
                    if bytes[index] != b'\n' {
                        masked[index] = b' ';
                    }
                    index += 1;
                }
            }
            State::String { quote, verbatim } => {
                if verbatim && bytes[index] == quote && bytes.get(index + 1) == Some(&quote) {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                } else if bytes[index] == quote {
                    masked[index] = b' ';
                    index += 1;
                    state = State::Code;
                } else if !verbatim && bytes[index] == b'\\' && index + 1 < bytes.len() {
                    masked[index] = b' ';
                    masked[index + 1] = b' ';
                    index += 2;
                } else {
                    if bytes[index] != b'\n' {
                        masked[index] = b' ';
                    }
                    index += 1;
                }
            }
        }
    }
    String::from_utf8(masked).expect("mask preserves source UTF-8 byte boundaries")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sts2_rules() -> Vec<ValidationRule> {
        vec![ValidationRule::ForbiddenCallInMethod {
            id: "sts2.energy.before_combat_start".into(),
            method_name: "BeforeCombatStart".into(),
            call_path: vec!["PlayerCmd".into(), "GainEnergy".into()],
            message: "ResetEnergy runs afterwards".into(),
        }]
    }

    #[test]
    fn rejects_gain_energy_inside_before_combat_start() {
        let source = r#"
public override async Task BeforeCombatStart()
{
    if (Owner != null)
    {
        await PlayerCmd
            . GainEnergy (1m, Owner);
    }
}
"#;
        let err = validate_generated_csharp(source, &sts2_rules()).unwrap_err();
        assert!(err.contains("ResetEnergy"));
        assert!(err.contains("sts2.energy.before_combat_start"));
    }

    #[test]
    fn accepts_evidenced_post_reset_hook() {
        let source = r#"
public override async Task AfterSideTurnStart(CombatSide side, CombatState combatState)
{
    if (side == Owner.Creature.Side && combatState.RoundNumber <= 1)
    {
        await PlayerCmd.GainEnergy(1m, Owner);
    }
}
"#;
        validate_generated_csharp(source, &sts2_rules()).unwrap();
    }

    #[test]
    fn ignores_comments_strings_and_calls_in_other_methods() {
        let source = r#"
public override Task BeforeCombatStart()
{
    // PlayerCmd.GainEnergy(1m, Owner);
    var text = "PlayerCmd.GainEnergy(1m, Owner)";
    return Task.CompletedTask;
}

public async Task Later()
{
    await PlayerCmd.GainEnergy(1m, Owner);
}
"#;
        validate_generated_csharp(source, &sts2_rules()).unwrap();
    }

    #[test]
    fn rejects_expression_bodied_method() {
        let source = "public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner);";
        assert!(validate_generated_csharp(source, &sts2_rules()).is_err());
    }

    #[test]
    fn rejects_method_declared_after_class_brace_on_same_line() {
        let source = "public class Relic { public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner); }";
        assert!(validate_generated_csharp(source, &sts2_rules()).is_err());
    }

    #[test]
    fn does_not_treat_method_invocation_as_declaration() {
        let source = r#"
public async Task Wrapper()
{
    await BeforeCombatStart();
    if (ready)
    {
        await PlayerCmd.GainEnergy(1m, Owner);
    }
}
"#;
        validate_generated_csharp(source, &sts2_rules()).unwrap();
    }

    #[test]
    fn source_is_not_rejected_when_pack_declares_no_rules() {
        let source = "public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner);";
        validate_generated_csharp(source, &[]).unwrap();
    }

    #[test]
    fn accepts_only_balanced_declared_localization_tags() {
        let allowed = vec!["blue".into(), "red".into()];
        validate_localization_rich_text(
            "Gain [blue]1[/blue] Energy and lose [red]2[/red] HP.",
            &allowed,
        )
        .unwrap();
        validate_localization_rich_text("Plain text.", &allowed).unwrap();
    }

    #[test]
    fn rejects_unknown_unbalanced_and_raw_square_bracket_tags() {
        let allowed = vec!["blue".into(), "red".into()];
        for value in [
            "Gain [yellow]1[/yellow] Energy.",
            "Gain [color=yellow]1[/color] Energy.",
            "Gain [blue]1 Energy.",
            "Gain [blue]1[/red] Energy.",
            "Gain [/blue]1 Energy.",
            "Use [1] charge.",
            "Use 1] charge.",
        ] {
            assert!(
                validate_localization_rich_text(value, &allowed).is_err(),
                "must reject {value}"
            );
        }
    }
}
