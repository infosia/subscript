//! Checks on the emitted C text and on trap consumption in LIR.

use super::*;

/// Reports every emitted type with no member, in one message
/// (`specs/blocks/compiler.md` §11d).
///
/// C11 6.7.2.1 gives a structure or a union at least one member and 6.7.2.2
/// gives an enumeration at least one enumerator. GCC and clang accept an
/// empty one as an extension; MSVC reports `C2016`, and an initializer on it
/// then reports `C2078`. The check reads the finished text and the standard
/// supplies the rule, so the two facts are derived apart (CLAUDE.md core
/// principle 9). It is total over the emitted output, so a new producer of
/// an empty type meets a build failure that names its site.
pub(super) fn verify_no_empty_aggregate(program: &CProgram) -> Result<(), String> {
    let mut found = Vec::new();
    for (text, source) in [
        ("the program", &program.source),
        (
            "the allocation metadata header",
            &program.allocation_metadata_header,
        ),
        (
            "the allocation metadata source",
            &program.allocation_metadata_source,
        ),
    ] {
        found.extend(empty_aggregates(text, source));
    }
    if found.is_empty() {
        return Ok(());
    }
    let sites = found
        .iter()
        .map(|site| {
            let name = if site.name.is_empty() {
                format!("an anonymous {}", site.keyword)
            } else {
                format!("{} {}", site.keyword, site.name)
            };
            format!("  {} line {}: {name}", site.text, site.line)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Err(internal(format!(
        "the emitted C declares {} type(s) with no member; C11 6.7.2.1 gives a \
         structure or a union at least one, and MSVC rejects an empty one \
         (compiler.md §11d):\n{sites}",
        found.len()
    )))
}

/// Whether `byte` can appear inside a C identifier.
fn is_identifier_byte(byte: Option<u8>) -> bool {
    matches!(byte, Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A copy of `source` where every comment, string literal, and character
/// literal becomes spaces of the same length. Newlines stay, so a byte index
/// into the result names the same line as in `source`. A brace inside a
/// comment or a literal is therefore not a member.
fn without_comments_and_literals(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match byte {
            b'/' if next == Some(b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    out.push(b' ');
                    index += 1;
                }
            }
            b'/' if next == Some(b'*') => {
                out.extend_from_slice(b"  ");
                index += 2;
                while index < bytes.len() {
                    if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        out.extend_from_slice(b"  ");
                        index += 2;
                        break;
                    }
                    out.push(if bytes[index] == b'\n' { b'\n' } else { b' ' });
                    index += 1;
                }
            }
            QUOTE_DOUBLE | QUOTE_SINGLE => {
                out.push(b' ');
                index += 1;
                while index < bytes.len() {
                    let inner = bytes[index];
                    if inner == b'\\' {
                        out.extend_from_slice(b"  ");
                        index += 2;
                        continue;
                    }
                    out.push(if inner == b'\n' { b'\n' } else { b' ' });
                    index += 1;
                    if inner == byte {
                        break;
                    }
                }
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    // An escape at the very end can push one byte past the source length.
    out.truncate(bytes.len());
    out.resize(bytes.len(), b' ');
    String::from_utf8(out).unwrap_or_else(|_| " ".repeat(bytes.len()))
}

/// The `"` that opens a string literal.
const QUOTE_DOUBLE: u8 = b'"';
/// The `'` that opens a character literal.
const QUOTE_SINGLE: u8 = 0x27;

/// The index just past any whitespace at `from`.
fn skip_whitespace(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() && bytes[from].is_ascii_whitespace() {
        from += 1;
    }
    from
}

/// The index of the `}` that closes the `{` at `open`, or `None` when the
/// text ends first.
fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every `struct`, `union`, or `enum` in `source` whose body holds nothing.
/// This is not a C parser: it reads the shape `keyword [tag] { ... }` and
/// asks whether the braces hold a member.
fn empty_aggregates(text: &'static str, source: &str) -> Vec<EmptyAggregate> {
    let scan = without_comments_and_literals(source);
    let bytes = scan.as_bytes();
    let mut found = Vec::new();
    for keyword in ["struct", "union", "enum"] {
        let mut from = 0;
        while let Some(offset) = scan[from..].find(keyword) {
            let start = from + offset;
            let after = start + keyword.len();
            from = after;
            let before = start.checked_sub(1).map(|index| bytes[index]);
            if is_identifier_byte(before) || is_identifier_byte(bytes.get(after).copied()) {
                continue;
            }
            let name_start = skip_whitespace(bytes, after);
            let mut cursor = name_start;
            while is_identifier_byte(bytes.get(cursor).copied()) {
                cursor += 1;
            }
            let name = scan[name_start..cursor].to_string();
            let open = skip_whitespace(bytes, cursor);
            if bytes.get(open) != Some(&b'{') {
                continue;
            }
            let Some(close) = matching_brace(bytes, open) else {
                continue;
            };
            if scan[open + 1..close].trim().is_empty() {
                found.push(EmptyAggregate {
                    text,
                    line: scan[..start].bytes().filter(|byte| *byte == b'\n').count() + 1,
                    keyword,
                    name,
                });
            }
        }
    }
    found.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then_with(|| left.keyword.cmp(right.keyword))
    });
    found
}

pub(super) fn verify_no_label_before_declaration(program: &CProgram) -> Result<(), String> {
    let mut found = Vec::new();
    for (text, source) in [
        ("the program", &program.source),
        (
            "the allocation metadata header",
            &program.allocation_metadata_header,
        ),
        (
            "the allocation metadata source",
            &program.allocation_metadata_source,
        ),
    ] {
        found.extend(labels_before_declarations(text, source));
    }
    if found.is_empty() {
        return Ok(());
    }
    let sites = found
        .iter()
        .map(|site| format!("  {} line {}: label `{}`", site.text, site.line, site.label))
        .collect::<Vec<_>>()
        .join("\n");
    Err(internal(format!(
        "the emitted C has {} label(s) followed directly by a declaration; C11 6.8.1 requires a statement after a label (compiler.md §11e):\n{sites}",
        found.len()
    )))
}

fn label_end(line: &str) -> Option<(usize, String)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    if let Some(rest) = trimmed.strip_prefix("case ") {
        let colon = rest.find(':')? + indent + "case ".len();
        return Some((colon + 1, line[indent..colon].trim().to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("default") {
        let whitespace = rest.len() - rest.trim_start().len();
        if rest.as_bytes().get(whitespace) == Some(&b':') {
            let colon = indent + "default".len() + whitespace;
            return Some((colon + 1, "default".to_string()));
        }
    }
    let name_len = trimmed
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    if name_len == 0 || trimmed.as_bytes().get(name_len) != Some(&b':') {
        return None;
    }
    Some((indent + name_len + 1, trimmed[..name_len].to_string()))
}

fn looks_like_declaration(statement: &str) -> bool {
    let statement = statement.trim_start();
    if statement.is_empty() || statement.starts_with([';', '{', '}']) {
        return false;
    }
    let first_len = statement
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    if first_len == 0 {
        return false;
    }
    let first = &statement[..first_len];
    if matches!(
        first,
        "break" | "continue" | "do" | "for" | "goto" | "if" | "return" | "switch" | "while"
    ) {
        return false;
    }
    let after_first = &statement[first_len..];
    if !after_first
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'*')
    {
        return false;
    }
    let candidate = after_first
        .trim_start()
        .trim_start_matches('*')
        .trim_start();
    candidate
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
}

fn labels_before_declarations(text: &'static str, source: &str) -> Vec<LabelBeforeDeclaration> {
    let scan = without_comments_and_literals(source);
    let lines = scan.lines().collect::<Vec<_>>();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((end, label)) = label_end(line) else {
            continue;
        };
        let mut statement = line[end..].trim_start();
        if statement.is_empty() {
            statement = lines
                .iter()
                .skip(index + 1)
                .map(|line| line.trim_start())
                .find(|line| !line.is_empty())
                .unwrap_or("");
        }
        if looks_like_declaration(statement) {
            found.push(LabelBeforeDeclaration {
                text,
                line: index + 1,
                label,
            });
        }
    }
    found
}

pub(super) fn runtime_traps(function: &l::Function) -> Vec<l::Trap> {
    let mut traps = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            traps.extend(instruction.traps.iter().cloned());
        }
        match &block.terminator {
            l::Terminator::Trap(trap) => traps.push(trap.clone()),
            l::Terminator::Unreachable { .. } => {}
            l::Terminator::Suspend { traps: sites, .. } => traps.extend(sites.iter().cloned()),
            l::Terminator::Branch(_)
            | l::Terminator::ConditionalBranch { .. }
            | l::Terminator::Switch { .. }
            | l::Terminator::Return { .. } => {}
        }
    }
    traps
}

pub(super) fn script_call_requires_pending_check(function: &l::Function) -> bool {
    function.is_generator || function.is_async || !runtime_traps(function).is_empty()
}

pub(super) fn verify_trap_consumption(
    function: &l::Function,
    expected: &[l::Trap],
    consumed: &[l::Trap],
) -> Result<(), String> {
    let mut matched = vec![false; consumed.len()];
    let mut missing = Vec::new();
    for trap in expected {
        if let Some(index) = consumed
            .iter()
            .zip(&matched)
            .position(|(candidate, matched)| !matched && candidate == trap)
        {
            matched[index] = true;
        } else {
            missing.push(trap);
        }
    }
    let extra = consumed
        .iter()
        .zip(matched)
        .filter_map(|(trap, matched)| (!matched).then_some(trap))
        .collect::<Vec<_>>();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }
    let site = missing
        .first()
        .copied()
        .or_else(|| extra.first().copied())
        .map_or(&function.pos, |trap| &trap.pos);
    Err(internal(format!(
        "function {} `{}` trap-consumption mismatch at {site}: LIR carries {} site(s), transcriber consumed {}; missing {missing:?}; extra {extra:?}",
        function.id.0,
        function.source_name,
        expected.len(),
        consumed.len()
    )))
}

#[cfg(test)]
mod empty_aggregate_tests {
    use super::*;

    fn sites(source: &str) -> Vec<String> {
        empty_aggregates("t", source)
            .into_iter()
            .map(|site| format!("{}:{}:{}", site.line, site.keyword, site.name))
            .collect()
    }

    #[test]
    fn an_empty_struct_is_reported_with_its_line_and_tag() {
        let source = "int a;\nstruct Frame {\n};\n";
        assert_eq!(sites(source), vec!["2:struct:Frame".to_string()]);
    }

    #[test]
    fn an_anonymous_empty_struct_is_reported_with_no_tag() {
        assert_eq!(sites("    struct {\n    } roots = {0};"), vec!["1:struct:"]);
    }

    /// The rule this check discharges is that one build reports every
    /// remaining site, so a later round never hunts for the next one
    /// (CLAUDE.md, two review rounds are the limit for a defect class).
    #[test]
    fn every_empty_type_is_reported_in_one_pass() {
        let source = "struct A {};\nunion B {  };\nenum C {\n};\nstruct D { int x; };\n";
        assert_eq!(sites(source), vec!["1:struct:A", "2:union:B", "3:enum:C"]);
    }

    #[test]
    fn a_declaration_without_a_body_is_not_a_site() {
        let source = concat!(
            "typedef struct subscript_rt_context subscript_rt_context;\n",
            "struct Forward;\n",
            "size_t n = sizeof(struct Forward);\n",
            "struct Full { int x; };\n",
            "restructure {};\n",
            "int structural = 0;\n"
        );
        assert!(sites(source).is_empty(), "{:?}", sites(source));
    }

    #[test]
    fn a_brace_pair_inside_a_comment_or_a_literal_is_not_a_body() {
        let source = concat!(
            "/* struct Commented {}; */\n",
            "// struct Lined {};\n",
            "const char* s = \"struct Quoted {}\";\n",
            "char c = \'{\';\n",
            "const char* e = \"a backslash \\\\\";\n",
            "struct Real { int x; };\n"
        );
        assert!(sites(source).is_empty(), "{:?}", sites(source));
    }

    /// A check whose "no sites" cannot be told from a broken check is worth
    /// nothing (CLAUDE.md core principle 9), so this perturbs a program and
    /// reads the message.
    #[test]
    fn the_program_check_names_every_site_and_the_standard() {
        let clean = CProgram {
            source: "struct Frame { int x; };\n".to_string(),
            positions: Vec::new(),
            allocation_metadata_header: String::new(),
            allocation_metadata_source: String::new(),
            foreign_symbols: Vec::new(),
        };
        verify_no_empty_aggregate(&clean).expect("a struct with a member is valid C");

        let perturbed = CProgram {
            source: "struct Frame {\n};\n".to_string(),
            allocation_metadata_header: "union Meta {};\n".to_string(),
            ..clean
        };
        let error = verify_no_empty_aggregate(&perturbed).expect_err("an empty struct is a defect");
        assert!(error.contains("2 type(s) with no member"), "{error}");
        assert!(
            error.contains("the program line 1: struct Frame"),
            "{error}"
        );
        assert!(
            error.contains("the allocation metadata header line 1: union Meta"),
            "{error}"
        );
        assert!(error.contains("C11 6.7.2.1"), "{error}");
    }
}

#[cfg(test)]
mod label_statement_tests {
    use super::*;

    #[test]
    fn label_check_reads_the_emitted_text_and_names_the_site() {
        let clean = CProgram {
            source: "resume_b6:\n    ;\n    SubFn t0 = frame->b6_v14;\n".to_string(),
            positions: Vec::new(),
            allocation_metadata_header: String::new(),
            allocation_metadata_source: String::new(),
            foreign_symbols: Vec::new(),
        };
        verify_no_label_before_declaration(&clean).expect("an empty statement satisfies C11 6.8.1");

        let perturbed = CProgram {
            source: "resume_b6: SubFn t0 = frame->b6_v14;\n".to_string(),
            ..clean
        };
        let error = verify_no_label_before_declaration(&perturbed)
            .expect_err("a declaration is not a statement");
        assert!(error.contains("1 label(s) followed directly"), "{error}");
        assert!(
            error.contains("the program line 1: label `resume_b6`"),
            "{error}"
        );
        assert!(error.contains("C11 6.8.1"), "{error}");
    }
}
