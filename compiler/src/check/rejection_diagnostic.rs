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
}

impl RejectionClass {
    pub(crate) fn divergence(self) -> Option<Divergence> {
        match self {
            Self::Diverges(variant) => Some(variant),
            Self::TscRejects => None,
        }
    }
}

pub(crate) fn diagnostic(site: RejectionSite, message: impl Into<String>, pos: Pos) -> Diagnostic {
    let message = message.into();
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
        diagnostic.note = Some("A function type has no optional parameter (C7); pass every argument, or call a declaration with a default directly.");
    }
    diagnostic
}
