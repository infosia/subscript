// corpus: reject/r271-re-export-alias-cycle/main
// purpose: Rejects an export alias cycle that reaches no declaration.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: rejects TS2303
// expected-error: S016 at line 8, the export specifier
// tsc-error-count: 1
export { x } from "./lib";
export function main(): void {}
