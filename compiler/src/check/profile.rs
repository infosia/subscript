//! The sandbox-profile source limits (`specs/blocks/compiler.md` §109.2,
//! S026), and the tests for every §109.2 rule.
//!
//! The parser's one lexer constructor refuses a source over the byte
//! limit, over every source file that is not ambient, before it hands
//! out a lexer (§109.2 rule 5). The limit is a count over bytes and
//! needs no lexer, so it is exact and total. The parser's stack holds
//! the deepest nesting a file of that size can spell (§109.2a), and the
//! exact nesting bound is the checker's guard (rule 2), after the parse.
//!
//! S023 to S025 report at their own sites, so this module holds no code
//! for them. It holds the pair every rule needs: the rejection under the
//! sandbox profile, and the firing control that the same source checks
//! clean under the default profile.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::{Profile, SourceFile};

use super::Checker;

/// The largest source file the sandbox profile accepts, in bytes
/// (§109.2, S026).
///
/// The count is over bytes and needs no lexer, so it is exact and total:
/// no construct of the grammar can spell more levels than the bytes of
/// this limit hold. The compile thread's stack holds the deepest nesting
/// a file of this size can spell;
/// `the_byte_limit_and_the_stack_hold_the_contract_margin` verifies that
/// against the measured parser cost of one level.
pub(crate) const SOURCE_BYTE_LIMIT: usize = 131_072;

/// The largest program the sandbox profile accepts, in bytes: the sum
/// over every file the entry imports, mirrors excluded.
pub(crate) const PROGRAM_BYTE_LIMIT: usize = 8_388_608;

/// The parser's stack cost for one level in the build that compiles this
/// code (§109.2a).
///
/// The cost is a measured record of the parser, not a number the
/// compiler reads: the byte limit and the compile thread's stack are the
/// two facts a compile uses, and
/// `the_byte_limit_and_the_stack_hold_the_contract_margin` compares them
/// against this record.
#[cfg(test)]
pub(crate) const fn parser_stack_bytes_per_level_worst() -> u64 {
    if cfg!(debug_assertions) {
        PARSER_STACK_BYTES_PER_LEVEL_WORST_UNOPTIMIZED
    } else {
        PARSER_STACK_BYTES_PER_LEVEL_WORST
    }
}

/// The worst product of parser stack cost and source density, in bytes
/// of stack for one level, in an optimized build (§109.2a).
///
/// The parenthesis is the worst construct: one source byte opens one
/// level. A file of [`SOURCE_BYTE_LIMIT`] bytes can therefore need
/// `SOURCE_BYTE_LIMIT` times this cost in bytes of stack.
#[cfg(test)]
pub(crate) const PARSER_STACK_BYTES_PER_LEVEL_WORST: u64 = 6_750;

/// The same cost in an unoptimized build, which is what the gate's own
/// binaries carry (§109.2a).
#[cfg(test)]
pub(crate) const PARSER_STACK_BYTES_PER_LEVEL_WORST_UNOPTIMIZED: u64 = 20_385;

/// The margin the compile thread's stack holds over the worst file the
/// byte limit admits, in tenths (§109.2a: 1.5).
#[cfg(test)]
pub(crate) const STACK_MARGIN_TENTHS: u64 = 15;

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

/// Every type one type node holds directly.
///
/// The match is exhaustive, so a new node kind of the pinned parser
/// fails the build here rather than escaping the descent.
fn child_types(ty: &ast::TsType) -> Vec<&ast::TsType> {
    fn of_annotation(annotation: &ast::TsTypeAnn) -> &ast::TsType {
        &annotation.type_ann
    }
    fn of_params<'a>(params: &'a [ast::TsFnParam], found: &mut Vec<&'a ast::TsType>) {
        for param in params {
            let annotation = match param {
                ast::TsFnParam::Ident(binding) => binding.type_ann.as_deref(),
                ast::TsFnParam::Array(pattern) => pattern.type_ann.as_deref(),
                ast::TsFnParam::Rest(pattern) => pattern.type_ann.as_deref(),
                ast::TsFnParam::Object(pattern) => pattern.type_ann.as_deref(),
            };
            found.extend(annotation.map(of_annotation));
        }
    }
    fn of_members<'a>(members: &'a [ast::TsTypeElement], found: &mut Vec<&'a ast::TsType>) {
        for member in members {
            match member {
                ast::TsTypeElement::TsCallSignatureDecl(signature) => {
                    of_params(&signature.params, found);
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
                ast::TsTypeElement::TsConstructSignatureDecl(signature) => {
                    of_params(&signature.params, found);
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
                ast::TsTypeElement::TsPropertySignature(signature) => {
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
                ast::TsTypeElement::TsGetterSignature(signature) => {
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
                ast::TsTypeElement::TsSetterSignature(signature) => {
                    of_params(std::slice::from_ref(&signature.param), found);
                }
                ast::TsTypeElement::TsMethodSignature(signature) => {
                    of_params(&signature.params, found);
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
                ast::TsTypeElement::TsIndexSignature(signature) => {
                    of_params(&signature.params, found);
                    found.extend(signature.type_ann.as_deref().map(of_annotation));
                }
            }
        }
    }
    fn of_type_param<'a>(param: &'a ast::TsTypeParam, found: &mut Vec<&'a ast::TsType>) {
        found.extend(param.constraint.as_deref());
        found.extend(param.default.as_deref());
    }

    let mut found: Vec<&ast::TsType> = Vec::new();
    match ty {
        ast::TsType::TsKeywordType(_) | ast::TsType::TsThisType(_) => {}
        ast::TsType::TsFnOrConstructorType(shape) => match shape {
            ast::TsFnOrConstructorType::TsFnType(function) => {
                of_params(&function.params, &mut found);
                found.push(of_annotation(&function.type_ann));
            }
            ast::TsFnOrConstructorType::TsConstructorType(constructor) => {
                of_params(&constructor.params, &mut found);
                found.push(of_annotation(&constructor.type_ann));
            }
        },
        ast::TsType::TsTypeRef(reference) => {
            if let Some(arguments) = &reference.type_params {
                found.extend(arguments.params.iter().map(Box::as_ref));
            }
        }
        ast::TsType::TsTypeQuery(query) => {
            if let Some(arguments) = &query.type_args {
                found.extend(arguments.params.iter().map(Box::as_ref));
            }
        }
        ast::TsType::TsTypeLit(literal) => of_members(&literal.members, &mut found),
        ast::TsType::TsArrayType(array) => found.push(&array.elem_type),
        ast::TsType::TsTupleType(tuple) => {
            found.extend(tuple.elem_types.iter().map(|element| &*element.ty));
        }
        ast::TsType::TsOptionalType(optional) => found.push(&optional.type_ann),
        ast::TsType::TsRestType(rest) => found.push(&rest.type_ann),
        ast::TsType::TsUnionOrIntersectionType(shape) => match shape {
            ast::TsUnionOrIntersectionType::TsUnionType(union) => {
                found.extend(union.types.iter().map(Box::as_ref));
            }
            ast::TsUnionOrIntersectionType::TsIntersectionType(intersection) => {
                found.extend(intersection.types.iter().map(Box::as_ref));
            }
        },
        ast::TsType::TsConditionalType(conditional) => {
            found.push(&conditional.check_type);
            found.push(&conditional.extends_type);
            found.push(&conditional.true_type);
            found.push(&conditional.false_type);
        }
        ast::TsType::TsInferType(infer) => of_type_param(&infer.type_param, &mut found),
        ast::TsType::TsParenthesizedType(parenthesized) => found.push(&parenthesized.type_ann),
        ast::TsType::TsTypeOperator(operator) => found.push(&operator.type_ann),
        ast::TsType::TsIndexedAccessType(access) => {
            found.push(&access.obj_type);
            found.push(&access.index_type);
        }
        ast::TsType::TsMappedType(mapped) => {
            of_type_param(&mapped.type_param, &mut found);
            found.extend(mapped.name_type.as_deref());
            found.extend(mapped.type_ann.as_deref());
        }
        ast::TsType::TsLitType(literal) => {
            if let ast::TsLit::Tpl(template) = &literal.lit {
                found.extend(template.types.iter().map(Box::as_ref));
            }
        }
        ast::TsType::TsTypePredicate(predicate) => {
            found.extend(predicate.type_ann.as_deref().map(of_annotation));
        }
        ast::TsType::TsImportType(import) => {
            if let Some(arguments) = &import.type_args {
                found.extend(arguments.params.iter().map(Box::as_ref));
            }
        }
    }
    found
}

impl Checker<'_> {
    /// Enters the nesting guard over every type of one type-parameter
    /// declaration: each constraint and each default (§109.2 rule 4).
    ///
    /// [`Checker::resolve_type`] guards every annotation it resolves. The
    /// types of a type-parameter declaration are TypeScript's own
    /// typing, which this language does not read, so no resolution walks
    /// them. This descent gives them the same bound. It resolves
    /// nothing, so it adds no diagnostic but S026 and creates no
    /// instance.
    ///
    /// Under the default profile it walks nothing, because the guard
    /// enforces nothing there.
    pub(crate) fn guard_type_parameters(&mut self, declaration: &ast::TsTypeParamDecl) {
        if self.profile != Profile::Sandbox {
            return;
        }
        for param in &declaration.params {
            for ty in param.constraint.iter().chain(param.default.iter()) {
                self.guard_type_depth(ty);
            }
        }
    }

    /// One level of the descent of [`Checker::guard_type_parameters`].
    fn guard_type_depth(&mut self, ty: &ast::TsType) {
        let pos = self.pos(ty.span());
        if self.enter_nesting(&pos) {
            for child in child_types(ty) {
                self.guard_type_depth(child);
            }
        }
        self.leave_nesting();
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

/// Reports the §109.2 S026 per-file byte limit that one source is over
/// (rule 5). The parser's one lexer constructor calls this, so no entry
/// parses a source this function rejects.
///
/// The count is over bytes and reads no token, so the check is exact and
/// total over the source. A count over tokens cannot be exact without
/// the parser: a lexer with no parser reads `/` as division, so a quote
/// or a bracket inside a regular-expression literal desyncs every later
/// token of the file.
pub(crate) fn source_limit_diagnostic(name: &str, bytes: usize) -> Option<Diagnostic> {
    if bytes <= SOURCE_BYTE_LIMIT {
        return None;
    }
    Some(Diagnostic::new(
        RuleCode::S026,
        format!(
            "source file of {bytes} bytes is over the sandbox profile limit of \
             {SOURCE_BYTE_LIMIT} bytes"
        ),
        Pos::new(name, 1, 1),
    ))
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

    /// The §109.2 rule 5 byte check of one program source, through the
    /// parser's one lexer constructor. The parser does not run.
    fn source_limit(source: &str) -> Option<Diagnostic> {
        crate::parse::sandbox_source_limit(&SourceFile::new("limits.ts", source))
            .into_iter()
            .next()
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
        assert!(source_limit(&at_limit).is_none());
        let over = "/".repeat(SOURCE_BYTE_LIMIT + 1);
        let diagnostic = source_limit(&over).expect("one byte over the limit reports");
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert_eq!(diagnostic.pos, Pos::new("limits.ts", 1, 1));
        assert!(
            diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
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

    /// §109.2a: the compile thread's stack holds the deepest nesting a
    /// file of [`SOURCE_BYTE_LIMIT`] bytes can spell, in each build, with
    /// the contract's margin of 1.5.
    ///
    /// The four contract numbers are pinned here and the margin is
    /// derived from them, so one constant that moves alone fails.
    #[test]
    fn the_byte_limit_and_the_stack_hold_the_contract_margin() {
        assert_eq!(SOURCE_BYTE_LIMIT, 131_072);
        assert_eq!(PROGRAM_BYTE_LIMIT, 8_388_608);
        assert_eq!(STACK_MARGIN_TENTHS, 15);
        assert_eq!(PARSER_STACK_BYTES_PER_LEVEL_WORST, 6_750);
        assert_eq!(PARSER_STACK_BYTES_PER_LEVEL_WORST_UNOPTIMIZED, 20_385);

        // The stack of the build this test runs in is the one the pair
        // names, and the cost selector answers that build's number.
        let (stack, cost) = if cfg!(debug_assertions) {
            (
                4_294_967_296_u64,
                PARSER_STACK_BYTES_PER_LEVEL_WORST_UNOPTIMIZED,
            )
        } else {
            (2_147_483_648_u64, PARSER_STACK_BYTES_PER_LEVEL_WORST)
        };
        assert_eq!(crate::COMPILE_THREAD_STACK_BYTES as u64, stack);
        assert_eq!(parser_stack_bytes_per_level_worst(), cost);

        // The worst file the byte limit admits, against the stack of the
        // build this test runs in.
        let worst = SOURCE_BYTE_LIMIT as u64 * cost;
        assert!(
            worst * STACK_MARGIN_TENTHS / 10 <= stack,
            "{worst} bytes at the limit, against a {stack}-byte stack"
        );
    }

    /// §109.2 rule 5: the byte check runs before the lexer exists, so a
    /// source over the limit reports S026 alone and no parse error of
    /// that source joins it.
    ///
    /// The nest here is 300,000 type-argument levels, which is deeper
    /// than the compile thread's stack holds in either build (§109.2a),
    /// and the unterminated string before it is a lexer error. The test
    /// returns, so the parser did not run.
    #[test]
    fn s026_reports_the_byte_limit_and_parses_nothing() {
        const LEVELS: usize = 300_000;
        let nest = format!("{}i32{}", "A<".repeat(LEVELS), ">".repeat(LEVELS));
        let source = format!("const s: string = \"x\nlet d: {nest};\n");
        assert!(
            source.len() > SOURCE_BYTE_LIMIT,
            "the source must be over the byte limit: {} bytes",
            source.len()
        );
        let reported = check(&source, Profile::Sandbox);
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert_eq!(reported[0].code, RuleCode::S026);
        assert!(
            reported[0].message.contains("bytes"),
            "{}",
            reported[0].message
        );
        // The firing control: the same shape under the byte limit parses,
        // and its lexer error reports.
        let short = format!(
            "const s: string = \"x\nlet d: {}i32{};\n",
            "A<".repeat(8),
            ">".repeat(8)
        );
        let parsed = check(&short, Profile::Sandbox);
        assert_eq!(parsed.len(), 1, "{parsed:?}");
        assert_eq!(parsed[0].code, RuleCode::S100);
    }

    #[test]
    fn s026_does_not_read_an_ambient_source() {
        let over = "/".repeat(SOURCE_BYTE_LIMIT + 1);
        let ambient = SourceFile::ambient("mirror.d.ts", over.clone());
        assert!(crate::parse::sandbox_source_limit(&ambient).is_empty());
        // The firing control: the same text in a program file reports.
        let program = SourceFile::new("program.ts", over);
        assert_eq!(crate::parse::sandbox_source_limit(&program).len(), 1);
    }

    /// §109.2 rule 2: the nesting guard is the one depth bound, so a
    /// parenthesis nest over it reports after the parse.
    #[test]
    fn s026_rejects_a_deep_source_under_the_sandbox_profile_only() {
        let diagnostic = sandbox_only_rejection(&nested(257));
        assert_eq!(diagnostic.code, RuleCode::S026);
        assert!(
            diagnostic.message.contains("nesting depth")
                && diagnostic.message.contains("sandbox profile"),
            "{}",
            diagnostic.message
        );
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
    /// The nesting guard is the one depth bound, so every shape reports
    /// through it, whether or not its level opens a bracket.
    #[test]
    fn s026_rejects_every_nesting_shape_over_the_limit() {
        for (name, source) in nesting_shapes(NESTING_DEPTH_LIMIT as usize + 1) {
            let sandbox = check(&source, Profile::Sandbox);
            assert_eq!(sandbox[0].code, RuleCode::S026, "{name}: {sandbox:?}");
            assert!(
                sandbox[0].message.contains("nesting depth")
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

    /// §109.2: the checker runs on the compile thread, so the depth a
    /// source can reach is a compiler fact and not the caller's thread.
    /// This test runs on an ordinary debug test thread, which holds about
    /// 65 levels on its own.
    #[test]
    fn the_compile_thread_carries_a_depth_no_caller_thread_holds() {
        for depth in [257, 2_000] {
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

    /// §109.2 rule 4: the work budget counts the type instances the
    /// checker creates, so a source that instantiates more often than
    /// the budget stops with S026.
    ///
    /// Measured with the test-only budget: the same source checks clean
    /// at 14 units with one instantiation and at 131 with ten, so each
    /// instance costs its own unit and the nodes of its body.
    #[test]
    fn s026_stops_the_check_at_the_instantiation_budget() {
        const TYPES: [&str; 10] = [
            "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64",
        ];
        let calls: String = TYPES
            .iter()
            .map(|ty| format!("  print(`${{identity<{ty}>(1 as {ty})}}`);\n"))
            .collect();
        let source = format!(
            "function identity<T>(value: T): T {{\n  return value;\n}}\n\nexport function main(): void {{\n{calls}}}\n"
        );
        let checked = |profile, budget| {
            let mut options = CheckOptions::with_profile(profile);
            options.budgets.work = budget;
            crate::on_the_compile_thread(|| {
                check_program_with(&[SourceFile::new("mono.ts", source.clone())], &options)
                    .err()
                    .unwrap_or_default()
            })
        };

        // Five units are fewer than the ten instances the source asks
        // for, so the instantiation charge alone stops the check.
        let reported = checked(Profile::Sandbox, 5);
        assert_eq!(reported[0].code, RuleCode::S026, "{reported:?}");
        assert!(
            reported[0]
                .message
                .contains("checker work over the sandbox profile budget"),
            "{}",
            reported[0].message
        );
        // The firing control: the raised budget accepts the same source,
        // and the default profile accepts it under the small budget.
        assert!(checked(Profile::Sandbox, 1_000).is_empty());
        assert!(checked(Profile::Default, 5).is_empty());
    }

    /// §109.2 rule 4: the nesting guard's type descent covers a
    /// type-parameter constraint as it covers an annotation. The
    /// declaration's default is the same node kind, so the guard covers
    /// it too.
    #[test]
    fn s026_rejects_a_deep_type_parameter_constraint_under_the_profile_only() {
        let nest = |levels: usize| format!("{}i32{}", "Array<".repeat(levels), ">".repeat(levels));
        let declared = |spelling: &str, levels: usize| {
            format!(
                "function identity<T {spelling} {}>(value: T): T {{\n  return value;\n}}\n\nexport function main(): void {{\n  print(`${{identity<i32>(1)}}`);\n}}\n",
                nest(levels)
            )
        };
        for spelling in ["extends", "="] {
            let source = |levels: usize| declared(spelling, levels);
            let over = source(257);
            let reported = check(&over, Profile::Sandbox);
            assert_eq!(reported[0].code, RuleCode::S026, "{reported:?}");
            assert!(
                reported[0].message.contains("nesting depth"),
                "{}",
                reported[0].message
            );
            // The firing control: the deepest accepted nest checks clean
            // under the profile, and the default profile accepts both.
            assert!(check(&source(254), Profile::Sandbox).is_empty());
            assert!(check(&over, Profile::Default).is_empty());
            assert!(check(&source(254), Profile::Default).is_empty());
        }
    }
}
