//! The sandbox-profile source limits (`specs/blocks/compiler.md` §109.2,
//! S026), and the tests for every §109.2 rule.
//!
//! The scan runs before the parser, over the bytes of every source file
//! that is not ambient. It does no lexing, so a bracket inside a string
//! or a comment counts. The count over-approximates the syntactic depth,
//! so it rejects more, never less.
//!
//! S023 to S025 report at their own sites, so this module holds no code
//! for them. It holds the pair every rule needs: the rejection under the
//! sandbox profile, and the firing control that the same source checks
//! clean under the default profile.

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::SourceFile;

/// The largest source file the sandbox profile accepts, in bytes.
pub(crate) const SOURCE_BYTE_LIMIT: usize = 1_048_576;

/// The largest bracket depth the sandbox profile accepts.
pub(crate) const BRACKET_DEPTH_LIMIT: u32 = 256;

/// Reports every source file that is over a §109.2 S026 limit.
///
/// One file reports at most one diagnostic: the byte limit first, then
/// the first bracket that takes the depth over the limit.
pub(crate) fn source_limit_diagnostics(files: &[SourceFile]) -> Vec<Diagnostic> {
    files
        .iter()
        .filter(|file| !file.dts)
        .filter_map(|file| source_limits(&file.name, &file.source))
        .collect()
}

/// Reports the first §109.2 S026 limit that one source is over.
fn source_limits(name: &str, source: &str) -> Option<Diagnostic> {
    let bytes = source.len();
    if bytes > SOURCE_BYTE_LIMIT {
        return Some(Diagnostic::new(
            RuleCode::S026,
            format!(
                "source file of {bytes} bytes is over the sandbox profile limit of \
                 {SOURCE_BYTE_LIMIT} bytes"
            ),
            Pos::new(name, 1, 1),
        ));
    }
    bracket_depth_limit(name, source)
}

/// Reports the first bracket that takes the depth over the limit.
///
/// The position counts the line breaks and the characters of the scan,
/// which is what the parser reports for the same offset.
fn bracket_depth_limit(name: &str, source: &str) -> Option<Diagnostic> {
    let mut depth: u32 = 0;
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for character in source.chars() {
        match character {
            '\n' => {
                line += 1;
                col = 0;
            }
            '(' | '[' | '{' => {
                depth += 1;
                if depth > BRACKET_DEPTH_LIMIT {
                    return Some(Diagnostic::new(
                        RuleCode::S026,
                        format!(
                            "bracket depth {depth} is over the sandbox profile limit of \
                             {BRACKET_DEPTH_LIMIT}"
                        ),
                        Pos::new(name, line, col),
                    ));
                }
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        col += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_program_with, CheckOptions, Profile};

    /// Checks one source under `profile` and answers its diagnostics.
    fn check(source: &str, profile: Profile) -> Vec<Diagnostic> {
        check_program_with(
            &[SourceFile::new("rule.ts", source)],
            &CheckOptions::with_profile(profile),
        )
        .err()
        .unwrap_or_default()
    }

    /// Answers the first diagnostic under the sandbox profile, and proves the
    /// same source checks clean under the default profile.
    fn sandbox_only_rejection(source: &str) -> Diagnostic {
        let clean = check(source, Profile::Default);
        assert!(
            clean.is_empty(),
            "the default profile must accept the source: {clean:?}"
        );
        let mut sandbox = check(source, Profile::Sandbox);
        assert!(
            !sandbox.is_empty(),
            "the sandbox profile must reject the source"
        );
        sandbox.remove(0)
    }

    const FREE: &str = "class Counter {\n  value: i32 = 0;\n}\n\nexport function main(): void {\n  const counter: Counter = new Counter();\n  Context.free(counter);\n}\n";

    const FROM_BYTES: &str = "@CStruct({ align: 4 })\nclass Point {\n  x: i32 = 0;\n}\n\nexport function main(): void {\n  const bytes: u8[] = [1, 0, 0, 0];\n  const point: Point = Context.fromBytes<Point>(bytes, 0);\n  print(`${point.x}`);\n}\n";

    const WORKER: &str = "class Message {\n  value: i32 = 0;\n}\n\nfunction entry(inbox: Inbox<Message>, outbox: Outbox<Message>): void {}\n\nexport function main(): void {\n  const worker: Worker<Message, Message> = Worker.spawn(entry);\n  worker.close();\n  worker.join();\n}\n";

    #[test]
    fn s023_rejects_context_free_under_the_sandbox_profile() {
        let diagnostic = sandbox_only_rejection(FREE);
        assert_eq!(diagnostic.code, RuleCode::S023);
        assert!(
            diagnostic.message.contains("sandbox profile")
                && diagnostic.message.contains("Context.free"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn s023_leaves_context_collect_callable_under_the_sandbox_profile() {
        let source = "export function main(): void {\n  Context.collect();\n}\n";
        assert!(check(source, Profile::Sandbox).is_empty());
    }

    #[test]
    fn s024_rejects_context_from_bytes_under_the_sandbox_profile() {
        let diagnostic = sandbox_only_rejection(FROM_BYTES);
        assert_eq!(diagnostic.code, RuleCode::S024);
        assert!(
            diagnostic.message.contains("sandbox profile")
                && diagnostic.message.contains("Context.fromBytes"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn s024_leaves_bytes_of_a_scalar_layout_callable_under_the_sandbox_profile() {
        let source = "@CStruct({ align: 4 })\nclass Point {\n  x: i32 = 0;\n}\n\nexport function main(): void {\n  const point: Point = new Point();\n  const bytes: u8[] = Context.bytesOf<Point>(point);\n  print(`${bytes.length}`);\n}\n";
        assert!(check(source, Profile::Sandbox).is_empty());
    }

    /// §109.2 S024's second clause has no reachable source. A `@CStruct`
    /// field of a reference class, a handle, or a string is outside the
    /// value-class whitelist, so the default profile rejects the declaration
    /// and the `Context.bytesOf` call before any profile rule runs.
    #[test]
    fn a_byte_layout_with_a_reference_field_is_rejected_by_the_default_profile() {
        let source = "class Payload {\n  value: i32 = 0;\n}\n\n@CStruct({ align: 8 })\nclass Holder {\n  count: i32 = 0;\n  payload: Payload = new Payload();\n}\n\nexport function main(): void {\n  const holder: Holder = new Holder();\n  const bytes: u8[] = Context.bytesOf<Holder>(holder);\n  print(`${bytes.length}`);\n}\n";
        let default = check(source, Profile::Default);
        assert!(
            default
                .iter()
                .any(|diagnostic| diagnostic.code == RuleCode::S100),
            "the default profile must reject the layout: {default:?}"
        );
        assert!(
            default
                .iter()
                .all(|diagnostic| diagnostic.code != RuleCode::S024),
            "no profile rule reaches this source"
        );
    }

    #[test]
    fn s025_rejects_the_worker_surface_under_the_sandbox_profile() {
        let diagnostic = sandbox_only_rejection(WORKER);
        assert_eq!(diagnostic.code, RuleCode::S025);
        assert!(
            diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn s025_rejects_worker_spawn_at_its_own_site() {
        // `Inbox` and `Outbox` name the entry's parameters, so the entry
        // reports first. This source names neither type.
        let source =
            "function entry(): void {}\n\nexport function main(): void {\n  Worker.spawn(entry);\n}\n";
        let sandbox = check(source, Profile::Sandbox);
        assert_eq!(sandbox[0].code, RuleCode::S025);
        assert!(
            sandbox[0].message.contains("`Worker.spawn`"),
            "{}",
            sandbox[0].message
        );
        // The firing control: the default profile rejects this entry for its
        // shape, not for the profile.
        let default = check(source, Profile::Default);
        assert!(default.iter().all(|d| d.code != RuleCode::S025));
    }

    #[test]
    fn the_default_profile_adds_no_diagnostic_for_any_rule_source() {
        for source in [FREE, FROM_BYTES, WORKER] {
            assert!(
                check(source, Profile::Default).is_empty(),
                "the default profile must accept every §109.2 rule source"
            );
        }
    }

    fn scan(source: &str) -> Option<Diagnostic> {
        source_limits("limits.ts", source)
    }

    /// One source of `depth` nested parentheses around a literal.
    fn nested(depth: usize) -> String {
        format!(
            "export function main(): void {{\n  const value: i32 = {}7{};\n  print(`${{value}}`);\n}}\n",
            "(".repeat(depth),
            ")".repeat(depth)
        )
    }

    #[test]
    fn s026_a_source_at_the_byte_limit_passes_and_one_byte_over_reports() {
        let at_limit = "/".repeat(SOURCE_BYTE_LIMIT);
        assert!(scan(&at_limit).is_none());
        let over = "/".repeat(SOURCE_BYTE_LIMIT + 1);
        let diagnostic = scan(&over).expect("one byte over the limit reports");
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert_eq!(diagnostic.pos, Pos::new("limits.ts", 1, 1));
        assert!(
            diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn s026_a_depth_at_the_limit_passes_and_one_over_reports_at_that_bracket() {
        let at_limit = format!(
            "{}0{}",
            "(".repeat(BRACKET_DEPTH_LIMIT as usize),
            ")".repeat(BRACKET_DEPTH_LIMIT as usize)
        );
        assert!(scan(&at_limit).is_none());
        let over = format!(
            "{}0{}",
            "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1),
            ")".repeat(BRACKET_DEPTH_LIMIT as usize + 1)
        );
        let diagnostic = scan(&over).expect("one level over the limit reports");
        assert_eq!(diagnostic.code, RuleCode::S026);
        // The 257th `(` is the character at column 257 of line 1.
        assert_eq!(
            diagnostic.pos,
            Pos::new("limits.ts", 1, BRACKET_DEPTH_LIMIT + 1)
        );
    }

    #[test]
    fn s026_a_closer_below_zero_resets_the_depth_to_zero() {
        // 256 unmatched closers precede the openers; without the reset the
        // scan would carry a negative depth and accept a deeper nest.
        let source = format!(
            "{}{}0",
            ")".repeat(BRACKET_DEPTH_LIMIT as usize),
            "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1)
        );
        let diagnostic = scan(&source).expect("the reset keeps the limit in force");
        assert_eq!(diagnostic.code, RuleCode::S026);
    }

    #[test]
    fn s026_counts_a_bracket_inside_a_string_or_a_comment() {
        let source = format!("// {}\n", "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1));
        let diagnostic = scan(&source).expect("the byte scan does no lexing");
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert_eq!(diagnostic.pos.line, 1);
    }

    #[test]
    fn s026_does_not_scan_an_ambient_source() {
        let deep = format!(
            "{}0{}",
            "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1),
            ")".repeat(BRACKET_DEPTH_LIMIT as usize + 1)
        );
        let ambient = SourceFile::ambient("mirror.d.ts", deep.clone());
        assert!(source_limit_diagnostics(&[ambient]).is_empty());
        // The firing control: the same text in a program file reports.
        let program = SourceFile::new("program.ts", deep);
        assert_eq!(source_limit_diagnostics(&[program]).len(), 1);
    }

    #[test]
    fn s026_rejects_a_deep_source_under_the_sandbox_profile_only() {
        let diagnostic = sandbox_only_rejection(&nested(BRACKET_DEPTH_LIMIT as usize + 1));
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert!(
            diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
    }

    /// §109.2: the checker runs on a 64 MiB thread, so the depth a source
    /// can reach is a compiler fact and not the caller's thread. This test
    /// runs on an ordinary debug test thread, which holds about 65 levels
    /// on its own.
    #[test]
    fn the_checker_thread_carries_a_depth_no_caller_thread_holds() {
        for depth in [BRACKET_DEPTH_LIMIT as usize + 1, 2_000] {
            let checked = crate::check_program(&[SourceFile::new("deep.ts", nested(depth))]);
            assert!(
                checked.is_ok(),
                "depth {depth} must check clean under the default profile: {:?}",
                checked.err()
            );
        }
    }
}
