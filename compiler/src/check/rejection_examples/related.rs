//! Examples shared by related diagnostics of the same fragment (§311 rule 8).
use super::*;

pub(super) fn example(site: RejectionSite) -> Option<&'static DivergenceEntry> {
    match site {
        RejectionSite::RestParameter => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(...args: i32[]): void {}",
                subscript: "function f(args: i32[]): void {}",
                ..Divergence::RestParameter.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::FunctionBodyMissing => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: i32): void; function f(x: i32): void {}",
                subscript: "function f(x: i32): void {}",
                ..Divergence::FunctionBodyMissing.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::GenericSourceAlias => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: Divergence::ArrayTypeArgument.entry().ts,
                subscript: Divergence::ArrayTypeArgument.entry().subscript,
                ..Divergence::GenericSourceAliasForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DefaultExportDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                why: "The module surface uses named exports and imports. Export the function under its name and import that name.",
                ts: Divergence::DefaultImport.entry().ts,
                subscript: Divergence::DefaultImport.entry().subscript,
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::InterfaceCallSignature => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: Divergence::NullableValueClassAssignment.entry().ts,
                subscript: Divergence::NullableValueClassAssignment.entry().subscript,
                ..Divergence::InterfaceMemberForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::SetClassInheritance => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: Divergence::SetAlgebraDomain.entry().ts,
                subscript: Divergence::SetAlgebraDomain.entry().subscript,
                ..Divergence::ReferenceClassInheritance.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayClassInheritance => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: Divergence::MapGroupByArraySource.entry().ts,
                subscript: Divergence::MapGroupByArraySource.entry().subscript,
                ..Divergence::ReferenceClassInheritance.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
