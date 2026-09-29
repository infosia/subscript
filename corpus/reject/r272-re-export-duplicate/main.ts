// corpus: reject/r272-re-export-duplicate/main
// purpose: Rejects two re-exports that use one export name.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: rejects TS2300
// expected-error: S017 at line 8, the export specifier

export { a as x, b as x } from "./lib";
export function main(): void {}
