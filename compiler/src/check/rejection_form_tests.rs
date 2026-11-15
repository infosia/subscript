//! Form-specific example checks (§312 table 312.1a).
use super::*;

/// Each table row checks an independently specified part of the rendered alternative.
/// Measured cost with --test-threads=1: 0.012796s.
#[test]
fn table_forms_show_their_own_alternative() {
    let started = Instant::now();
    let cases = [
        (RejectionSite::ParameterCapture, Divergence::MutableLocalCapture, "function f(param: i32): void { const read = (): i32 => param; }", "const v = param;"),
        (RejectionSite::LaterLocalCaptureRead, Divergence::DeclarationScope, "function f(): void { const read = (): i32 => x; const x: i32 = 4; }", "const x: i32 = 4; const read"),
        (RejectionSite::SourceAmbientFunctionDeclaration, Divergence::FunctionBodyMissing, "declare function f(): void;", "mirror.d.ts"),
        (RejectionSite::ExportAllDeclaration, Divergence::NamedModuleSurface, "// file: other.ts\nexport function n(): void {}\n// file: main.ts\nexport * from \"./other\";", "export { n }"),
        (RejectionSite::GenericUserAlias, Divergence::GenericSourceAliasForm, "type A<T> = \"a\" | \"b\";", "type IntList = i32[];"),
        (RejectionSite::GenericUserAlias, Divergence::GenericSourceAliasForm, "type List<T> = T[];", "type IntList = i32[];"),
        (RejectionSite::GenericUserAlias, Divergence::GenericSourceAliasForm, "type Pair<T> = [T, T];", "type IntList = i32[];"),
        (RejectionSite::MirrorStringModuleDeclaration, Divergence::MirrorModuleDeclarationForm, "// file: mirror.d.ts\ndeclare module \"x\" { var n: i32; }\n// file: main.ts\nexport function main(): void {}", "declare const n"),
        (RejectionSite::WeakMapConstructor, Divergence::LibConstructorName, "class Key {} new WeakMap<Key, i32>();", "new Map<"),
        (RejectionSite::WeakMapConstructor, Divergence::LibConstructorName, "class Key {} new WeakSet<Key>();", "new Map<"),
        (RejectionSite::WeakMapConstructor, Divergence::LibConstructorName, "class Key {} const v = new Key(); new WeakRef<Key>(v);", "new Map<"),
        (RejectionSite::ValueArraySearch, Divergence::MethodTypeDomain, "@ValueType class Box { x: i32 = 1; }\nfunction f(xs: Box[], v: Box): boolean { return xs.includes(v); }", "for (const x of xs)"),
        (RejectionSite::CallbackParameterNeedsConstName, Divergence::CallbackFewerParametersUnadapted, "function g(fv: () => void): void { const xs: i32[] = [1]; xs.forEach(fv); }", "const f = fv;"),
        (RejectionSite::FunctionParameterNeedsConstName, Divergence::FunctionValueAdapterNeedsConstName, "function run(cb: (x: i32, y: i32) => void): void { cb(1, 2); }\nfunction g(fv: (x: i32) => void): void { run(fv); }", "const f = fv;"),
        (RejectionSite::ArrayCallbackDropsAsyncHandle, Divergence::DroppedAsyncHandle, "async function work(x: i32): Promise<void> {}\nfunction f(xs: i32[]): void { xs.forEach(async (x: i32): Promise<void> => { await work(x); }); }", "for (const x of xs)"),
        (RejectionSite::IntegerToEnumAssignment, Divergence::ErasedAssignableTypeMismatch, "enum E { A, B }\nfunction f(n: i32): E { const y: E = n; return y; }", "switch (n)"),
        (RejectionSite::PromiseFinallyAsyncResult, Divergence::PromiseReactionResult, "async function f(): Promise<i32> { return 1; }\nasync function g(): Promise<void> {}\nasync function main(): Promise<void> { await f().finally(g); }", "finally { await g(); }"),
        (RejectionSite::PromiseRejectionParameter, Divergence::PromiseReactionParameter, "async function f(): Promise<i32> { return 1; }\nasync function main(): Promise<void> { await f().catch((e: string): i32 => 0); }", "(e: Error): i32 => 0"),
        (RejectionSite::ErrorMemberWriteOutsideSurface, Divergence::ErrorMemberOutsideSurface, "const e = new Error(\"x\"); e.cause = \"s\";", "const cause ="),
        (RejectionSite::TypeofLiteral, Divergence::TypeofOperator, "const t = typeof 1;", "const t = \"number\""),
        (RejectionSite::FileModuleReexport, Divergence::FileModuleImportForm, "export { writeFile } from \"node:fs/promises\";", "export async function save"),
        (RejectionSite::TextModuleReexport, Divergence::TextModuleImportForm, "export { graphemeLength } from \"subscript:text\";", "export function length"),
    ];
    let mut failures = Vec::new();
    for (site, variant, source, expected) in cases {
        let files = fragment_files(variant, source);
        let options = crate::CheckOptions {
            enabled_modules: vec!["node:fs/promises".to_owned()],
            ..crate::CheckOptions::default()
        };
        clear_reached();
        let diagnostics = crate::check_program_with(&files, &options)
            .err()
            .unwrap_or_default();
        let reached = take_reached();
        let diagnostic = diagnostics.iter().find(|diagnostic| {
            reached.iter().any(|(actual, message, pos)| {
                *actual == site && *message == diagnostic.message && *pos == diagnostic.pos
            })
        });
        let shown = diagnostic.and_then(|d| {
            d.divergence
                .map(|v| d.example.copied().unwrap_or_else(|| v.entry()))
        });
        if !shown.is_some_and(|entry| entry.subscript.contains(expected)) {
            failures.push(format!(
                "{site:?}: expected {expected:?}; shown {:?}",
                shown.map(|entry| entry.subscript)
            ));
        } else if let Some(diagnostic) = diagnostic {
            let rendered = crate::render_diagnostics(&files, std::slice::from_ref(diagnostic));
            assert!(
                rendered.contains(expected),
                "{site:?}: rendered alternative absent"
            );
        }
    }
    eprintln!("s312: table forms {:.6}s", started.elapsed().as_secs_f64());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A checker input with only the entry models a CLI that does not load dynamic targets.
/// Measured cost with --test-threads=1: 0.014641s.
#[test]
fn dynamic_import_without_loaded_target_keeps_its_site() {
    let started = Instant::now();
    for (source, expected_message, expected_col) in [
        (
            "import(\"./other\");",
            "call form outside the decided surface",
            1,
        ),
        (
            "async function f(): Promise<void> { const m = await import(\"./other\"); }",
            "awaitable expressions must be direct calls",
            47,
        ),
    ] {
        for loaded in [false, true] {
            let mut files = vec![SourceFile::entry("main.ts", source)];
            if loaded {
                files.push(SourceFile::new("other.ts", "export function n(): void {}"));
            }
            clear_reached();
            let diagnostics = check_program(&files).unwrap_err();
            let reached = take_reached();
            assert!(
                reached
                    .iter()
                    .any(|(site, _, _)| *site == RejectionSite::DynamicImportCall),
                "loaded={loaded}: {reached:?}"
            );
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.message == expected_message)
                .unwrap();
            assert_eq!(diagnostic.pos, Pos::new("main.ts", 1, expected_col));
            assert_eq!(diagnostic.code.to_string(), "S100");
            assert_eq!(diagnostic.divergence, Some(Divergence::DynamicImportCall));
            assert!(
                crate::render_diagnostics(&files, std::slice::from_ref(diagnostic))
                    .contains("= TypeScript accepts:")
            );
        }
    }
    eprintln!(
        "s312: dynamic import {:.6}s",
        started.elapsed().as_secs_f64()
    );
}
