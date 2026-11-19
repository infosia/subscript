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
        RejectionSite::WorkerEntryGenerator => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class M { x: i32 = 1; } function* entry(i: Inbox<M>, o: Outbox<M>): Generator<i32> { yield 1; } export function main(): void { Worker.spawn<M,M>(entry); }",
                subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} export function main(): void { Worker.spawn<M,M>(entry); }",
                why: "Calling a generator creates a suspended frame instead of executing the worker body. Use a synchronous function that runs the work.",
                ..Divergence::WorkerEntrySignature.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WorkerEntryAsyncResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): Promise<void> { throw new Error(\"pending\"); } export function main(): void { Worker.spawn<M,M>(entry); }",
                subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} export function main(): void { Worker.spawn<M,M>(entry); }",
                why: "An async handle result needs an await. A worker discards its entry result, so the result must hold no async handle.",
                ..Divergence::WorkerEntrySignature.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WorkerEntryDefaultPrefix => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class M { x: i32 = 1; static inbox(): Inbox<M> { throw new Error(\"default\"); } } function entry(i: Inbox<M> = M.inbox()): void {} export function main(): void { Worker.spawn<M,M>(entry); }",
                subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} export function main(): void { Worker.spawn<M,M>(entry); }",
                why: "An entry with default parameters cannot adapt to additional callback parameters. Declare both endpoint parameters.",
                ..Divergence::WorkerEntrySignature.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
