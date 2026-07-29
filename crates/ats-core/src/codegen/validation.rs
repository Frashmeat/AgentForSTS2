use regex::Regex;

pub(crate) struct ForbiddenCallInMethodRule {
    pub id: &'static str,
    pub method_name: &'static str,
    pub call_path: &'static [&'static str],
    pub message: &'static str,
}

const STS2_RULES: &[ForbiddenCallInMethodRule] = &[ForbiddenCallInMethodRule {
    id: "sts2.energy.before_combat_start",
    method_name: "BeforeCombatStart",
    call_path: &["PlayerCmd", "GainEnergy"],
    message: "known STS2 timing error: BeforeCombatStart calls PlayerCmd.GainEnergy, but SetupPlayerTurn.ResetEnergy runs afterwards. Use the current official implementation and lifecycle caller evidence to choose a post-reset hook, with Owner-side and first-round guards where required",
}];

pub(crate) fn validate_sts2_generated_csharp(source: &str) -> Result<(), String> {
    validate_forbidden_calls_in_methods(source, STS2_RULES)
}

fn validate_forbidden_calls_in_methods(
    source: &str,
    rules: &[ForbiddenCallInMethodRule],
) -> Result<(), String> {
    let masked = mask_csharp_comments_and_literals(source);
    for rule in rules {
        let method_pattern = Regex::new(&format!(
            r"(?m)^\s*(?:public|protected|internal|private)\s+(?:(?:static|virtual|override|async|sealed|new)\s+)*[A-Za-z_][\w<>,\.\[\]\?]*\s+{}\s*\(",
            regex::escape(rule.method_name)
        ))
        .expect("static method rule must compile");
        let call_pattern = Regex::new(&format!(
            r"\b{}\s*\(",
            rule.call_path
                .iter()
                .map(|part| regex::escape(part))
                .collect::<Vec<_>>()
                .join(r"\s*\.\s*")
        ))
        .expect("static call rule must compile");

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
                    rule.id, rule.message
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
        let err = validate_sts2_generated_csharp(source).unwrap_err();
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
        validate_sts2_generated_csharp(source).unwrap();
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
        validate_sts2_generated_csharp(source).unwrap();
    }

    #[test]
    fn rejects_expression_bodied_method() {
        let source = "public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner);";
        assert!(validate_sts2_generated_csharp(source).is_err());
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
        validate_sts2_generated_csharp(source).unwrap();
    }
}
