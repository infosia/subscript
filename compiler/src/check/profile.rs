//! The sandbox-profile source limits (`specs/blocks/compiler.md` §109.2,
//! S026), and the tests for every §109.2 rule.
//!
//! The scan runs before the parser, over every source file that is not
//! ambient. The depth is a count over the **lexer's tokens**, so a
//! bracket inside a comment, a string, a template, or a
//! regular-expression literal is not a bracket. The lexer is a flat loop
//! over the bytes, so the cost of the scan does not grow with the depth.
//!
//! S023 to S025 report at their own sites, so this module holds no code
//! for them. It holds the pair every rule needs: the rejection under the
//! sandbox profile, and the firing control that the same source checks
//! clean under the default profile.

use swc_ecma_parser::token::Token;

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::parse;
use crate::{Profile, SourceFile};

use super::Checker;

/// The largest source file the sandbox profile accepts, in bytes.
pub(crate) const SOURCE_BYTE_LIMIT: usize = 1_048_576;

/// The largest program the sandbox profile accepts, in bytes: the sum
/// over every file the entry imports, mirrors excluded.
pub(crate) const PROGRAM_BYTE_LIMIT: usize = 8_388_608;

/// The largest bracket depth the sandbox profile accepts.
pub(crate) const BRACKET_DEPTH_LIMIT: u32 = 256;

/// The largest number of lexer tokens the sandbox profile accepts in one
/// file (§109.2, S026).
///
/// The bracket count stops a nest that a bracket opens. Type arguments,
/// prefix operators, conditional expressions, and assignment chains open
/// no bracket, and the parser recurses once for each of their levels
/// before any checker rule can run. Every parser level consumes at least
/// one token, so this count is the proxy that bounds the parser's own
/// recursion.
///
/// The number is one contract number, the same in every build. It is
/// `stack / (cost × margin)`, rounded down to a power of two, and the
/// stack of each build profile is the size that answers it
/// (§109.2a). [`token_count_limit`] derives it from the pair of the
/// build it compiles in, and a test compares the derived value against
/// the contract's for both pairs, so no constant moves alone.
pub(crate) const TOKEN_COUNT_LIMIT: u32 = token_count_limit();

/// Derives the token limit from the compile thread's stack and the
/// parser cost of this build (§109.2a).
const fn token_count_limit() -> u32 {
    let per_level = parser_stack_bytes_per_level() * TOKEN_LIMIT_MARGIN_TENTHS;
    let levels = (crate::COMPILE_THREAD_STACK_BYTES as u64 * 10) / per_level;
    1_u32 << levels.ilog2()
}

/// The parser's stack cost for one level in the build that compiles this
/// code (§109.2a).
pub(crate) const fn parser_stack_bytes_per_level() -> u64 {
    if cfg!(debug_assertions) {
        PARSER_STACK_BYTES_PER_LEVEL_UNOPTIMIZED
    } else {
        PARSER_STACK_BYTES_PER_LEVEL
    }
}

/// The parser's stack cost for one level of the deepest construct the
/// bracket count does not bound, in an optimized build, in bytes
/// (§109.2a: type arguments).
pub(crate) const PARSER_STACK_BYTES_PER_LEVEL: u64 = 5_313;

/// The same cost in an unoptimized build, which is what the gate's own
/// binaries carry (§109.2a).
pub(crate) const PARSER_STACK_BYTES_PER_LEVEL_UNOPTIMIZED: u64 = 16_018;

/// The margin the token limit holds over the parser cost of its build,
/// in tenths (§109.2a: 1.5).
///
/// One level costs at least one token, so the margin covers the levels a
/// construct with a cheaper token cost can reach.
pub(crate) const TOKEN_LIMIT_MARGIN_TENTHS: u64 = 15;

/// The deepest recursive descent the sandbox profile accepts
/// (§109.2 rule 2). One expression, one type, or one statement is one
/// level.
pub(crate) const NESTING_DEPTH_LIMIT: u32 = 256;

/// The largest frame the sandbox profile accepts, in bytes (§109.2,
/// S027).
pub(crate) const FRAME_BYTE_LIMIT: u64 = 65_536;

/// The §109.2 rule 4 budgets.
///
/// The default values are the contract's. A test lowers them, because no
/// source inside S026's limits reaches the contract's numbers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Budgets {
    /// Nodes visited plus type instances created, over the whole check.
    pub work: u64,
}

impl Budgets {
    /// The contract's budgets (§109.2 rule 4). The LIR output budget is
    /// the lowering's, and it lives with the lowering.
    pub(crate) const CONTRACT: Self = Self { work: 16_777_216 };
}

impl Default for Budgets {
    fn default() -> Self {
        Self::CONTRACT
    }
}

impl Checker<'_> {
    /// Enters one level of the checker's recursive descent (§109.2
    /// rule 2), and spends one unit of the work budget (rule 4).
    ///
    /// Answers `false` when this node is over the sandbox profile's
    /// nesting limit, or when a budget already stopped the check. The
    /// caller then answers its own error form. Every caller calls
    /// [`Checker::leave_nesting`] afterwards, whatever this answers.
    ///
    /// Under the default profile the guard counts and enforces nothing.
    pub(crate) fn enter_nesting(&mut self, pos: &Pos) -> bool {
        self.nesting_depth += 1;
        if !self.spend_work(1, pos) {
            return false;
        }
        if self.profile != Profile::Sandbox || self.nesting_depth <= NESTING_DEPTH_LIMIT {
            return true;
        }
        self.error(
            RuleCode::S026,
            format!(
                "nesting depth {} is over the sandbox profile limit of \
                 {NESTING_DEPTH_LIMIT}",
                self.nesting_depth
            ),
            pos.clone(),
        );
        false
    }

    /// Leaves the level that [`Checker::enter_nesting`] entered.
    pub(crate) fn leave_nesting(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }

    /// Spends `units` of the §109.2 rule 4 work budget.
    ///
    /// Answers `false` once the budget is spent, and reports S026 at the
    /// node that crossed it. Every later call answers `false`, so the
    /// check stops. Under the default profile it counts and enforces
    /// nothing.
    pub(crate) fn spend_work(&mut self, units: u64, pos: &Pos) -> bool {
        self.work = self.work.saturating_add(units);
        if self.profile != Profile::Sandbox {
            return true;
        }
        if self.budget_stopped {
            return false;
        }
        if self.work <= self.work_budget {
            return true;
        }
        self.budget_stopped = true;
        self.error(
            RuleCode::S026,
            format!(
                "checker work over the sandbox profile budget of {} units",
                self.work_budget
            ),
            pos.clone(),
        );
        false
    }
}

/// Reports the program-wide byte limit (§109.2, S026), before the parser.
///
/// The sum is over every file that is not ambient: a mirror is the host's
/// text, not the program's.
pub(crate) fn program_limit_diagnostic(files: &[SourceFile]) -> Option<Diagnostic> {
    let bytes: usize = files
        .iter()
        .filter(|file| !file.dts)
        .map(|file| file.source.len())
        .sum();
    if bytes <= PROGRAM_BYTE_LIMIT {
        return None;
    }
    let entry = files.iter().find(|file| !file.dts)?;
    Some(Diagnostic::new(
        RuleCode::S026,
        format!(
            "program of {bytes} bytes is over the sandbox profile limit of \
             {PROGRAM_BYTE_LIMIT} bytes"
        ),
        Pos::new(entry.name.clone(), 1, 1),
    ))
}

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
    token_limits(name, source)
}

/// Reports the first token that takes the bracket depth or the token
/// count over its limit.
///
/// The bracket count is over the lexer's tokens: `(`, `[`, `{`, and the
/// `${` of a template head each open one level, and `)`, `]`, and `}`
/// each close one. `${` opens a level because the `}` that ends its
/// expression is an ordinary `}` token, and because the parser recurses
/// once for the expression it opens. A closer below zero resets the depth
/// to zero. A lexer error stops the scan and reports nothing; the parser
/// reports that source.
///
/// The token count is over every token the lexer answers. It bounds the
/// parser's recursion for the nests that open no bracket.
///
/// One flat loop reads both, so the cost of the scan does not grow with
/// the depth. The first limit in token order reports.
///
/// The position is the token's own span, which is what the parser
/// reports for the same offset.
fn token_limits(name: &str, source: &str) -> Option<Diagnostic> {
    parse::with_tokens(name, source, |tokens, at| {
        let mut depth: u32 = 0;
        let mut count: u32 = 0;
        for spanned in tokens {
            if matches!(spanned.token, Token::Error(_)) {
                break;
            }
            count += 1;
            if count > TOKEN_COUNT_LIMIT {
                return Some(Diagnostic::new(
                    RuleCode::S026,
                    format!(
                        "source file of {count} tokens is over the sandbox profile limit of \
                         {TOKEN_COUNT_LIMIT} tokens"
                    ),
                    at(spanned.span.lo),
                ));
            }
            match spanned.token {
                Token::LParen | Token::LBracket | Token::LBrace | Token::DollarLBrace => {
                    depth += 1;
                    if depth > BRACKET_DEPTH_LIMIT {
                        return Some(Diagnostic::new(
                            RuleCode::S026,
                            format!(
                                "bracket depth {depth} is over the sandbox profile limit of \
                                 {BRACKET_DEPTH_LIMIT}"
                            ),
                            at(spanned.span.lo),
                        ));
                    }
                }
                Token::RParen | Token::RBracket | Token::RBrace => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        None
    })
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_program_with, CheckOptions, Profile};

    /// Checks one source under `profile` and answers its diagnostics.
    ///
    /// §109.2 rule 3: every caller of the checker wraps the compile, so
    /// the depth a source can reach is the compile thread's and not this
    /// test thread's.
    fn check(source: &str, profile: Profile) -> Vec<Diagnostic> {
        crate::on_the_compile_thread(|| {
            check_program_with(
                &[SourceFile::new("rule.ts", source)],
                &CheckOptions::with_profile(profile),
            )
            .err()
            .unwrap_or_default()
        })
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

    /// The shape the external review measured: a byte count cancels
    /// itself, because the `)` inside each comment decrements it. The
    /// token count sees one `(` per level and no closer.
    fn comment_levels(levels: usize) -> String {
        format!(
            "export function main(): void {{\n  const x: i32 = {}1{};\n  print(`${{x}}`);\n}}\n",
            "(/*)*/".repeat(levels),
            ")".repeat(levels)
        )
    }

    #[test]
    fn s026_counts_the_levels_that_a_byte_count_cancels() {
        let diagnostic = sandbox_only_rejection(&comment_levels(257));
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert_eq!(diagnostic.pos.line, 2);
        assert!(
            diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
        // The firing control: the same shape under the limit checks clean.
        // The function body's own `{` holds one level, so 255 levels is
        // the deepest this shape reaches at the limit.
        assert!(scan(&comment_levels(255)).is_none());
    }

    #[test]
    fn s026_does_not_count_a_bracket_inside_a_comment_or_a_literal() {
        let openers = "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1);
        let escaped = r"\(".repeat(BRACKET_DEPTH_LIMIT as usize + 1);
        for source in [
            format!("// {openers}\n"),
            format!("/* {openers} */\n"),
            format!("const s: string = \"{openers}\";\n"),
            format!("const t: string = `{openers}`;\n"),
            format!("const r: RegExp = /{escaped}/;\n"),
        ] {
            assert!(
                scan(&source).is_none(),
                "the token scan counts no bracket here: {source}"
            );
        }
        // The firing control: the same openers as tokens report.
        assert!(scan(&openers).is_some());
    }

    #[test]
    fn s026_counts_a_template_substitution_as_one_opener() {
        // `${` opens a brace token in SWC, and the `}` that ends the
        // expression closes it, so each level counts exactly once.
        let nest = |levels: usize| {
            format!(
                "{}1{}",
                "`${".repeat(levels),
                "}`".repeat(levels).to_string()
            )
        };
        assert!(scan(&nest(BRACKET_DEPTH_LIMIT as usize)).is_none());
        let diagnostic =
            scan(&nest(BRACKET_DEPTH_LIMIT as usize + 1)).expect("257 `${` levels report");
        assert_eq!(diagnostic.code, RuleCode::S026);
    }

    /// §109.2a: a refused compile thread is a rejection under the
    /// profile, because every bound the profile holds assumes that
    /// stack. The control is the same source under the default profile,
    /// which keeps the inline fallback and checks clean.
    #[test]
    fn a_refused_compile_thread_reports_s026_under_the_profile() {
        const SOURCE: &str = "export function main(): void {\n  print(\"x\");\n}\n";
        let refused = |profile| {
            // The hook is thread-local, so it refuses the spawn this
            // call makes and no other.
            let _forced = crate::ForcedRefusal::enter();
            crate::on_the_compile_thread(|| {
                check_program_with(
                    &[SourceFile::new("refused.ts", SOURCE)],
                    &CheckOptions::with_profile(profile),
                )
                .err()
                .unwrap_or_default()
            })
        };
        let sandbox = refused(Profile::Sandbox);
        assert_eq!(sandbox.len(), 1, "{sandbox:?}");
        assert_eq!(sandbox[0].code, RuleCode::S026);
        assert!(
            sandbox[0].message.contains("compile thread is unavailable")
                && sandbox[0].message.contains("sandbox profile"),
            "{}",
            sandbox[0].message
        );
        // The control: the default profile runs the work inline.
        assert!(refused(Profile::Default).is_empty());
        // The control: the profile accepts the same source when the
        // thread spawns.
        assert!(check(SOURCE, Profile::Sandbox).is_empty());
    }

    /// §109.2a: the derived limit is the contract's number, and the
    /// three constants it reads are the contract's measured pair and its
    /// margin. A change to one of them alone fails here, so the
    /// derivation cannot drift.
    #[test]
    fn the_token_limit_follows_from_the_stack_and_the_cost() {
        assert_eq!(TOKEN_COUNT_LIMIT, 131_072);
        assert_eq!(TOKEN_LIMIT_MARGIN_TENTHS, 15);
        assert_eq!(PARSER_STACK_BYTES_PER_LEVEL, 5_313);
        assert_eq!(PARSER_STACK_BYTES_PER_LEVEL_UNOPTIMIZED, 16_018);

        // The stack of the build this test runs in is the one the pair
        // names, and the limit above is what that pair derives.
        let (stack, cost) = if cfg!(debug_assertions) {
            (4_294_967_296_u64, PARSER_STACK_BYTES_PER_LEVEL_UNOPTIMIZED)
        } else {
            (1_073_741_824_u64, PARSER_STACK_BYTES_PER_LEVEL)
        };
        assert_eq!(crate::COMPILE_THREAD_STACK_BYTES as u64, stack);
        assert_eq!(parser_stack_bytes_per_level(), cost);

        // Both pairs derive the one contract number, each with a margin
        // of at least 1.5. One constant that moves alone fails here.
        for (build, stack, cost) in [
            ("optimized", 1_073_741_824_u64, PARSER_STACK_BYTES_PER_LEVEL),
            (
                "unoptimized",
                4_294_967_296_u64,
                PARSER_STACK_BYTES_PER_LEVEL_UNOPTIMIZED,
            ),
        ] {
            let levels = (stack * 10) / (cost * TOKEN_LIMIT_MARGIN_TENTHS);
            let derived = 1_u64 << levels.ilog2();
            assert_eq!(derived, u64::from(TOKEN_COUNT_LIMIT), "{build}");
            let held = u64::from(TOKEN_COUNT_LIMIT) * cost;
            assert!(
                held * TOKEN_LIMIT_MARGIN_TENTHS / 10 <= stack,
                "{build}: {held} bytes at the limit, against a {stack}-byte stack"
            );
        }
    }

    /// §109.2 S026: the token count bounds the parser's recursion for
    /// every nest that opens no bracket.
    #[test]
    fn s026_a_token_count_at_the_limit_passes_and_one_over_reports() {
        // One empty statement is one token, so this is the cheapest
        // token flood a source can hold.
        let at_limit = ";".repeat(TOKEN_COUNT_LIMIT as usize);
        assert!(scan(&at_limit).is_none());
        let over = ";".repeat(TOKEN_COUNT_LIMIT as usize + 1);
        let diagnostic = scan(&over).expect("one token over the limit reports");
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert!(
            diagnostic.message.contains("tokens"),
            "{}",
            diagnostic.message
        );
        // The firing control: the same source under the default profile
        // checks clean.
        assert!(check(&over, Profile::Default).is_empty());
        assert_eq!(check(&over, Profile::Sandbox)[0].code, RuleCode::S026);
    }

    #[test]
    fn s026_reports_the_byte_limit_before_the_depth() {
        let deep = format!(
            "{}0{}",
            "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1),
            ")".repeat(BRACKET_DEPTH_LIMIT as usize + 1)
        );
        let padding = " ".repeat(SOURCE_BYTE_LIMIT + 1 - deep.len());
        let diagnostic = scan(&format!("{deep}{padding}")).expect("a source over both reports");
        assert!(
            diagnostic.message.contains("bytes"),
            "the byte limit reports first: {}",
            diagnostic.message
        );
        // The firing control: the same brackets under the byte limit
        // report the depth.
        assert!(scan(&deep)
            .expect("the depth reports on its own")
            .message
            .contains("bracket depth"));
    }

    #[test]
    fn s026_reports_nothing_for_a_source_the_lexer_rejects() {
        let openers = "(".repeat(BRACKET_DEPTH_LIMIT as usize + 1);
        // The unterminated string stops the lexer before the openers.
        assert!(scan(&format!("const s: string = \"x\n{openers}")).is_none());
        // The firing control: the terminated string leaves the openers
        // in the token stream.
        assert!(scan(&format!("const s: string = \"x\";\n{openers}")).is_some());
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

    /// §109.2: the lexer is a flat loop over the bytes, so the scan
    /// returns from a small stack at a depth no parser reaches. The
    /// thread here is 2 MiB, the size of an ordinary test thread.
    #[test]
    fn s026_scans_a_deep_source_from_a_two_mebibyte_stack() {
        let sources = [
            ("12,000 comment levels", comment_levels(12_000)),
            (
                "12,000 template openers",
                format!("{}1{}", "`${".repeat(12_000), "}`".repeat(12_000)),
            ),
            // 58,254 lines of 18 bytes is 1,048,572: the largest source
            // under the byte limit, so the lexer runs on all of it.
            ("1 MiB of source", "const x: i32 = 1;\n".repeat(58_254)),
        ];
        let scanned = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                for (name, source) in sources {
                    let started = std::time::Instant::now();
                    let reported = scan(&source).is_some();
                    println!(
                        "{name}: {} bytes, {:?}, reported={reported}",
                        source.len(),
                        started.elapsed()
                    );
                }
            })
            .expect("spawn the scan thread");
        scanned.join().expect("the scan returns from a 2 MiB stack");
    }

    /// One `main` whose single declaration nests `count` levels of one
    /// construct (§109.2 rule 2).
    ///
    /// The declaration statement is one level and the leaf is one more,
    /// so a chain of `count` reaches depth `count + 2`.
    fn nesting_shapes(count: usize) -> [(&'static str, String); 4] {
        let mut arrows = String::from("1");
        for _ in 0..count {
            arrows = format!("((): i32 => {arrows})()");
        }
        [
            (
                "type arguments",
                format!(
                    "export function main(): void {{\n  const deep: {}i32{} = [];\n  print(`${{deep.length}}`);\n}}\n",
                    "Array<".repeat(count),
                    ">".repeat(count)
                ),
            ),
            (
                "prefix `!`",
                format!(
                    "export function main(): void {{\n  const flag: boolean = {}true;\n  print(`${{flag}}`);\n}}\n",
                    "!".repeat(count)
                ),
            ),
            (
                "conditional `? :`",
                format!(
                    "export function main(): void {{\n  const value: i32 = {}1;\n  print(`${{value}}`);\n}}\n",
                    "true ? 1 : ".repeat(count)
                ),
            ),
            ("arrow bodies", format!(
                "export function main(): void {{\n  const value: i32 = {arrows};\n  print(`${{value}}`);\n}}\n"
            )),
        ]
    }

    /// §109.2 rule 2: each of these four shapes is over the limit at 257
    /// levels, and each checks clean under the default profile.
    ///
    /// The first three open no bracket token, so the nesting guard is the
    /// one limit that stops them. An arrow level opens a parenthesis, so
    /// the bracket count reaches 257 first and reports there.
    #[test]
    fn s026_rejects_every_nesting_shape_over_the_limit() {
        for (index, (name, source)) in nesting_shapes(NESTING_DEPTH_LIMIT as usize + 1)
            .into_iter()
            .enumerate()
        {
            let sandbox = check(&source, Profile::Sandbox);
            assert_eq!(sandbox[0].code, RuleCode::S026, "{name}: {sandbox:?}");
            let limit = if index == 3 {
                "bracket depth"
            } else {
                "nesting depth"
            };
            assert!(
                sandbox[0].message.contains(limit)
                    && sandbox[0].message.contains("sandbox profile"),
                "{name}: {}",
                sandbox[0].message
            );
            // The firing control: the same source under the default
            // profile checks clean, so the limit is the profile's.
            let default = check(&source, Profile::Default);
            assert!(default.is_empty(), "{name}: {default:?}");
        }
    }

    /// §109.2 rule 2: the deepest form of each shape that the profile
    /// accepts. The counts are measured: the declaration statement and
    /// the leaf are levels of their own, and one arrow level is two.
    #[test]
    fn s026_accepts_every_nesting_shape_at_the_limit() {
        for (deepest, index) in [(254, 0), (254, 1), (254, 2), (127, 3)] {
            let (name, source) = &nesting_shapes(deepest)[index];
            let sandbox = check(source, Profile::Sandbox);
            assert!(sandbox.is_empty(), "{name} at {deepest}: {sandbox:?}");
            // The firing control: one level more reports.
            let (_, over) = &nesting_shapes(deepest + 1)[index];
            let reported = check(over, Profile::Sandbox);
            assert_eq!(
                reported[0].code,
                RuleCode::S026,
                "{name} at {}: {reported:?}",
                deepest + 1
            );
        }
    }

    /// §109.2 rule 2: the descent stops at the node that is over the
    /// limit, so the diagnostic count does not grow with the chain.
    ///
    /// One node reports for each child the last accepted node holds, so a
    /// conditional chain reports three and a unary chain reports one. A
    /// descent that did not stop would report once per level.
    #[test]
    fn s026_reports_a_count_that_does_not_grow_with_the_depth() {
        for (index, (name, shallow)) in nesting_shapes(500).into_iter().enumerate() {
            let (_, deep) = &nesting_shapes(2_000)[index];
            let shallow = check(&shallow, Profile::Sandbox);
            let deep = check(deep, Profile::Sandbox);
            assert!(
                shallow.iter().all(|entry| entry.code == RuleCode::S026),
                "{name}: {shallow:?}"
            );
            assert_eq!(
                shallow.len(),
                deep.len(),
                "{name}: 500 levels reported {} and 2,000 levels reported {}",
                shallow.len(),
                deep.len()
            );
        }
    }

    /// §109.2 rule 4: the work budget stops the check and reports S026 at
    /// the node that crossed it.
    #[test]
    fn s026_stops_the_check_at_the_work_budget() {
        let source = "export function main(): void {\n  let total: i32 = 0;\n  total = total + 1;\n  print(`${total}`);\n}\n";
        let mut options = CheckOptions::with_profile(Profile::Sandbox);
        options.budgets.work = 4;
        let reported = crate::on_the_compile_thread(|| {
            check_program_with(&[SourceFile::new("budget.ts", source)], &options)
                .err()
                .unwrap_or_default()
        });
        assert_eq!(reported[0].code, RuleCode::S026, "{reported:?}");
        assert!(
            reported[0]
                .message
                .contains("checker work over the sandbox profile budget"),
            "{}",
            reported[0].message
        );
        // The firing control: the contract budget accepts the same source,
        // and the default profile accepts it under the small budget.
        assert!(check(source, Profile::Sandbox).is_empty());
        let mut default = CheckOptions::default();
        default.budgets.work = 4;
        assert!(crate::on_the_compile_thread(|| {
            check_program_with(&[SourceFile::new("budget.ts", source)], &default).is_ok()
        }));
    }

    /// §109.2 S026: the program limit is the byte sum over every file the
    /// entry imports, and it reports one time, at the entry file, before
    /// the parser.
    #[test]
    fn s026_reports_the_program_byte_limit_once_at_the_entry() {
        const FIVE_MEBIBYTES: usize = 5 * 1024 * 1024;
        let entry = format!(
            "export function main(): void {{\n  print(\"program\");\n}}\n// {}\n",
            "x".repeat(FIVE_MEBIBYTES)
        );
        let other = format!("// {}\n", "y".repeat(FIVE_MEBIBYTES));
        let files = [
            SourceFile::new("entry.ts", entry.clone()),
            SourceFile::new("other.ts", other.clone()),
        ];
        let reported = crate::on_the_compile_thread(|| {
            check_program_with(&files, &CheckOptions::with_profile(Profile::Sandbox))
                .err()
                .unwrap_or_default()
        });
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert_eq!(reported[0].code, RuleCode::S026);
        assert_eq!(reported[0].pos, Pos::new("entry.ts", 1, 1));
        assert!(
            reported[0].message.contains("program of"),
            "{}",
            reported[0].message
        );
        // The firing control: two files of half the limit are under it,
        // and the per-file limit then reports each one instead.
        let quarter = PROGRAM_BYTE_LIMIT / 4;
        let small = [
            SourceFile::new("entry.ts", format!("// {}\n", "x".repeat(quarter))),
            SourceFile::new("other.ts", format!("// {}\n", "y".repeat(quarter))),
        ];
        assert!(program_limit_diagnostic(&small).is_none());
        // A mirror is the host's text, so it is outside the sum.
        let with_mirror = [
            SourceFile::new("entry.ts", entry),
            SourceFile::ambient("mirror.d.ts", other),
        ];
        assert!(program_limit_diagnostic(&with_mirror).is_none());
    }

    /// §109.2 S027: the profile lowers the frame limit to 65,536 bytes.
    #[test]
    fn s027_rejects_a_frame_over_the_limit_under_the_profile_only() {
        let frame = |bytes: usize| {
            format!(
                "function probe(input: FixedArray<u8, {bytes}>): u8 {{\n  const block: FixedArray<u8, {bytes}> = input;\n  return block[0];\n}}\n\nexport function main(): void {{\n  print(\"frame\");\n}}\n"
            )
        };
        let over = frame(FRAME_BYTE_LIMIT as usize + 1);
        let sandbox = check(&over, Profile::Sandbox);
        assert_eq!(sandbox.len(), 1, "{sandbox:?}");
        assert_eq!(sandbox[0].code, RuleCode::S027);
        assert!(
            sandbox[0].message.contains("sandbox profile"),
            "{}",
            sandbox[0].message
        );
        // The diagnostic names the function, not the local.
        assert_eq!(sandbox[0].pos.line, 1);
        // §109.7: a profile rejection is not a TypeScript divergence.
        assert!(sandbox[0].divergence.is_none(), "{:?}", sandbox[0]);
        // The firing control: the same source under the default profile
        // checks clean, and the limit itself is accepted under both.
        assert!(check(&over, Profile::Default).is_empty());
        let at_limit = frame(FRAME_BYTE_LIMIT as usize);
        assert!(check(&at_limit, Profile::Sandbox).is_empty());
    }

    /// §109.2: the checker runs on a 64 MiB thread, so the depth a source
    /// can reach is a compiler fact and not the caller's thread. This test
    /// runs on an ordinary debug test thread, which holds about 65 levels
    /// on its own.
    #[test]
    fn the_compile_thread_carries_a_depth_no_caller_thread_holds() {
        for depth in [BRACKET_DEPTH_LIMIT as usize + 1, 2_000] {
            let checked = crate::on_the_compile_thread(|| {
                crate::check_program(&[SourceFile::new("deep.ts", nested(depth))])
            });
            assert!(
                checked.is_ok(),
                "depth {depth} must check clean under the default profile: {:?}",
                checked.err()
            );
        }
    }

    /// §109.2 rule 3: a nested call is already on the compile thread, so
    /// it runs the work inline and spawns nothing.
    #[test]
    fn a_nested_compile_thread_call_runs_inline() {
        let (outer, inner) = crate::on_the_compile_thread(|| {
            let outer = std::thread::current().id();
            let inner = crate::on_the_compile_thread(|| std::thread::current().id());
            (outer, inner)
        });
        assert_eq!(outer, inner);
        // The firing control: an outer call from this test thread moves
        // the work to a thread of its own.
        let spawned = crate::on_the_compile_thread(|| std::thread::current().id());
        assert_ne!(spawned, std::thread::current().id());
    }
}
