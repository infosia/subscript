//! Examples for each split form (§312 rule 1).
use super::*;

pub(super) fn example(site: RejectionSite) -> Option<&'static DivergenceEntry> {
    match site {
        RejectionSite::ParameterCapture => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(param: i32): void { const read = (): i32 => param; }",
                subscript: "function f(param: i32): void { const v = param; const read = (): i32 => v; }",
                why: "A closure copies const locals. Copy the parameter into a const before the closure.",
                ..Divergence::MutableLocalCapture.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::LaterLocalCaptureRead => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(): void { const read = (): i32 => x; const x: i32 = 4; }",
                subscript: "function f(): void { const x: i32 = 4; const read = (): i32 => x; }",
                why: "A closure cannot read a later local. Declare the local before the closure.",
                ..Divergence::DeclarationScope.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::SourceAmbientFunctionDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "declare function f(): void;",
                subscript: "// file: mirror.d.ts\n// @subscript-c-header include=\"host.h\"\ndeclare function f(): void;\n// file: main.ts\nexport function main(): void {}",
                why: "A source function requires a body. Put the host declaration in a mirror with its C header.",
                ..Divergence::FunctionBodyMissing.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ExportAllDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: other.ts\nexport function n(): void {}\n// file: main.ts\nexport * from \"./other\";",
                subscript: "// file: other.ts\nexport function n(): void {}\n// file: main.ts\nexport { n } from \"./other\";",
                why: "The module surface requires explicit named exports. List each exported name.",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::GenericUserAlias => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type List<T> = T[]; function f(x: List<i32>): void {}",
                subscript: "type IntList = i32[]; function f(x: IntList): void {}",
                why:
                    "A source alias has no type parameters. Declare an alias for the concrete type.",
                ..Divergence::GenericSourceAliasForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MirrorStringModuleDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\ndeclare module \"x\" { var n: i32; }\n// file: main.ts\nexport function main(): void {}",
                subscript: "// file: mirror.d.ts\n// @subscript-c-header include=\"host.h\"\ndeclare const n = 1;\n// file: main.ts\nexport function main(): void {}",
                why: "A mirror declares C header items at the top level. Remove the ambient module wrapper.",
                ..Divergence::MirrorModuleDeclarationForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WeakMapConstructor => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class Key {} new WeakMap<Key, i32>();",
                subscript: "class Key {} new Map<Key, i32>();",
                why: "The deterministic container surface has no WeakMap, WeakSet, or WeakRef. Use an explicitly owned Map.",
                ..Divergence::LibConstructorName.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ValueArraySearch => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class Box { x: i32 = 1; }\nfunction f(xs: Box[], v: Box): boolean { return xs.includes(v); }",
                subscript: "@ValueType class Box { x: i32 = 1; }\nfunction f(xs: Box[], v: Box): boolean { for (const x of xs) { if (x.x === v.x) { return true; } } return false; }",
                why: "A value array has no intrinsic equality search. Compare the fields in a loop.",
                collision: "compiler.md §176",
            };
            Some(&ENTRY)
        }
        RejectionSite::CallbackParameterNeedsConstName => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function g(fv: () => void): void { const xs: i32[] = [1]; xs.forEach(fv); }",
                subscript: "function g(fv: () => void): void { const xs: i32[] = [1]; const f = fv; xs.forEach(f); }",
                why: "A parameter function value has no adapter name. Copy it into a const before the callback place.",
                ..Divergence::CallbackFewerParametersUnadapted.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::FunctionParameterNeedsConstName => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function run(cb: (x: i32, y: i32) => void): void { cb(1, 2); }\nfunction g(fv: (x: i32) => void): void { run(fv); }",
                subscript: "function run(cb: (x: i32, y: i32) => void): void { cb(1, 2); }\nfunction g(fv: (x: i32) => void): void { const f = fv; run(f); }",
                why: "A parameter function value has no adapter name. Copy it into a const before the wider place.",
                ..Divergence::FunctionValueAdapterNeedsConstName.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayCallbackDropsAsyncHandle => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function work(x: i32): Promise<void> {}\nfunction f(xs: i32[]): void { xs.forEach(async (x: i32): Promise<void> => { await work(x); }); }",
                subscript: "async function work(x: i32): Promise<void> {}\nasync function f(xs: i32[]): Promise<void> { for (const x of xs) { await work(x); } }",
                why: "forEach drops the async handle. Await each operation in a for-of loop.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::IntegerToEnumAssignment => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "enum E { A, B }\nfunction f(n: i32): E { const y: E = n; return y; }",
                subscript: "enum E { A, B }\nfunction f(n: i32): E { switch (n) { case 0: return E.A; case 1: return E.B; default: throw new Error(\"invalid enum\"); } }",
                why: "An integer assignment does not check enum membership. Select a declared member with a switch.",
                ..Divergence::ErasedAssignableTypeMismatch.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseFinallyAsyncResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): Promise<i32> { return 1; }\nasync function g(): Promise<void> {}\nasync function main(): Promise<void> { await f().finally(g); }",
                subscript: "async function f(): Promise<i32> { return 1; }\nasync function g(): Promise<void> {}\nasync function main(): Promise<void> { try { await f(); } finally { await g(); } }",
                why: "A finally callback cannot return an async handle. Await the cleanup in a finally statement.",
                ..Divergence::PromiseReactionResult.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseRejectionParameter => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): Promise<i32> { return 1; }\nasync function main(): Promise<void> { await f().catch((e: string): i32 => 0); }",
                subscript: "async function f(): Promise<i32> { return 1; }\nasync function main(): Promise<void> { await f().catch((e: Error): i32 => 0); }",
                why: "A rejection callback receives Error. Use Error as its parameter type.",
                ..Divergence::PromiseReactionParameter.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ErrorMemberWriteOutsideSurface => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const e = new Error(\"x\"); e.cause = \"s\";",
                subscript: "const e = new Error(\"x\"); const cause = \"s\";",
                why: "An Error has no writable cause or stack field. Keep the extra data in a separate binding.",
                ..Divergence::ErrorMemberOutsideSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TypeofLiteral => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const t = typeof 1;",
                subscript: "const t = \"number\";",
                why:
                    "Types are static. Replace the literal typeof expression with its known string.",
                ..Divergence::TypeofOperator.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::FileModuleReexport => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "export { writeFile } from \"node:fs/promises\";",
                subscript: "// file: main.ts\nimport { save } from \"./files\";\n// file: files.ts\nimport { writeFile } from \"node:fs/promises\";\nexport async function save(path: string, data: string): Promise<void> { await writeFile(path, data); }",
                why: "The file module exposes named imports. Export a wrapper that calls the imported function.",
                ..Divergence::FileModuleImportForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TextModuleReexport => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "export { graphemeLength } from \"subscript:text\";",
                subscript: "// file: main.ts\nimport { length } from \"./text\";\n// file: text.ts\nimport { graphemeLength } from \"subscript:text\";\nexport function length(text: string): i32 { return graphemeLength(text); }",
                why: "The text module exposes named imports. Export a wrapper that calls the imported function.",
                ..Divergence::TextModuleImportForm.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
