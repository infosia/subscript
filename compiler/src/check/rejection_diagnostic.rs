//! Diagnostic emission retains the named rejection site.
use super::{Checker, Diagnostic, Divergence, Pos, RejectionSite};

impl Checker<'_> {
    pub(in crate::check) fn reject_subset(
        &mut self,
        site: RejectionSite,
        message: impl Into<String>,
        pos: Pos,
    ) {
        self.diags.push(diagnostic(site, message, pos));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RejectionClass {
    Diverges(Divergence),
    TscRejects,
    // A host input error, not a subset rejection (compiler.md §267 rule 5).
    ProgramInput,
}

impl RejectionClass {
    pub(crate) fn divergence(self) -> Option<Divergence> {
        match self {
            Self::Diverges(variant) => Some(variant),
            Self::TscRejects | Self::ProgramInput => None,
        }
    }
}

pub(crate) fn diagnostic(site: RejectionSite, message: impl Into<String>, pos: Pos) -> Diagnostic {
    // A message names a declaration by its source spelling: a hidden
    // local function name (compiler.md §207.1 rule 3) and every other
    // identity marker leave the text.
    let message = crate::hir::source_name(&message.into());
    #[cfg(test)]
    crate::check::rejection_total::record_site(site, &message, &pos);
    let (code, class) = site.class();
    let mut diagnostic = Diagnostic::new(code, message, pos);
    diagnostic.divergence = class.divergence();
    if diagnostic
        .message
        .contains("a generator is a counted type (§176)")
    {
        diagnostic.example = Some(&crate::divergence::DivergenceEntry {
            ts: "values.forEach(value => value.next());",
            subscript: "for (const value of values) { value.next(); }",
            why: "A generator is a counted type; callback positions do not carry its owner counts.",
            collision: "compiler.md §176",
        });
    }
    if matches!(
        site,
        RejectionSite::FunctionValueArgumentCount
            | RejectionSite::FunctionValueOptionalArgumentCount
    ) {
        diagnostic.note =
            Some("Pass every argument, or call a declaration with a default directly.");
    }
    match site {
        RejectionSite::Api(crate::ambient::ApiRejectionId::DateLocalGetYear, _) => diagnostic.note = Some("Use `date.getUTCFullYear()` to read the UTC year."),
        RejectionSite::Api(crate::ambient::ApiRejectionId::FormGroupBy, _) => diagnostic.note = Some("Object groups have no admitted type. Use `Map.groupBy(items, callback)`."),
        RejectionSite::ContextBytesTypeCount => diagnostic.note = Some("Each byte conversion has one target type. Use `Context.bytesOf<T>(value)`, `Context.bytesInto<T>(value, target, offset)`, or `Context.fromBytes<T>(bytes, offset)`."),
        RejectionSite::NumberMethodArgumentCount => diagnostic.note = Some("Use zero or one argument for `x.toFixed(digits)` and `x.toExponential(digits)`, or exactly one for `x.toString(radix)` and `x.toPrecision(digits)`."),
        RejectionSite::ContextBytesArgumentCount => diagnostic.note = Some("Use one argument for `Context.bytesOf<T>(value)`, three for `Context.bytesInto<T>(value, target, offset)`, or two for `Context.fromBytes<T>(bytes, offset)`."),
        RejectionSite::ArrayFromArgumentCount => diagnostic.note = Some("Use exactly one source argument: `Array.from(source)`."),
        RejectionSite::NumberPredicateCount => diagnostic.note = Some("Use exactly one f64 argument: `Number.isFinite(value)`, `Number.isNaN(value)`, `Number.isInteger(value)`, or `Number.isSafeInteger(value)`."),
        RejectionSite::JsonParseCount => diagnostic.note = Some("Use exactly one text argument: `JSON.parse<T>(text)`."),
        RejectionSite::JsonParseTypeCount => diagnostic.note = Some("JSON parsing has one target type. Use `JSON.parse<T>(text)`."),
        RejectionSite::ForOfKeys => diagnostic.note = Some("FixedArray has no iterator view methods. Iterate values with `for (const value of items)`, or indices with `for (let i: i32 = 0; i < items.length; i++)`."),
        RejectionSite::NullableMemberLoopHead
        | RejectionSite::NullableCallLoopHead
        | RejectionSite::NullableNominalAssignmentLoopHead => diagnostic.note = Some("The narrowing before the loop does not hold in the loop, because the loop stores this location. Copy the value into a `const` local before the loop. Or test the value for null again before this use."),
        RejectionSite::MethodValueTruthTest => diagnostic.note = Some("A method is never null. Call it without the test."),
        RejectionSite::IncompatibleNominalAssignment => diagnostic.note = Some("Classes are nominal. Make an object of the declared class."),
        RejectionSite::DisjointLiteralAliasAssignment => diagnostic.note = Some("Assign a literal member of the target alias."),
        RejectionSite::ImmediateShadowedNameRead => diagnostic.note = Some("A later local declaration hides the outer name. Give the local another name."),
        RejectionSite::ClassReadBeforeDeclaration => diagnostic.note = Some("Move the declaration before the code that uses it."),
        RejectionSite::TaskGroupGeneratorBody => {
            diagnostic.example = Some(&crate::check::diagnostic_text::GROUP);
            diagnostic.rule = Some("A generator body cannot use a TaskGroup because a dropped iterator has no scope exit.");
        }
        RejectionSite::PromiseReactionSpread => diagnostic.example = Some(&crate::check::diagnostic_text::REACTION_SPREAD),
        RejectionSite::DateMethodValue => diagnostic.example = Some(&crate::check::diagnostic_text::DATE),
        RejectionSite::AsyncArrowCapture => diagnostic.rule = Some("An async arrow can own const captures. Copy a mutable value into a const or use a class field."),
        RejectionSite::AwaitIndirectCall => diagnostic.note = Some("Call the synchronous function without await, or await a function that returns an async handle."),
        _ => {}
    }
    diagnostic
}

pub(crate) fn async_receiver_diagnostic(pos: Pos) -> Diagnostic {
    let mut diagnostic = diagnostic(
        RejectionSite::AsyncArrowCapture,
        crate::check::diagnostic_text::RECEIVER_MESSAGE,
        pos,
    );
    diagnostic.example = Some(&crate::check::diagnostic_text::RECEIVER);
    diagnostic.rule = Some(crate::check::diagnostic_text::RECEIVER_RULE);
    diagnostic
}

/// A synchronous reaction callback that captures `this` (§186 rule 4).
pub(crate) fn reaction_receiver_diagnostic(method: &str, pos: Pos) -> Diagnostic {
    let mut diagnostic = diagnostic(
        RejectionSite::AsyncArrowCapture,
        format!("a `.{method}(...)` callback cannot capture `this`; copy the needed field into a `const` first"),
        pos,
    );
    diagnostic.example = Some(&crate::check::diagnostic_text::REACTION_RECEIVER);
    diagnostic.rule = Some(crate::check::diagnostic_text::REACTION_RECEIVER_RULE);
    diagnostic
}
