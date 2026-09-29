// corpus: reject/r270-re-export-missing/main
// purpose: Rejects a re-export whose source module does not export the requested name.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: rejects TS2305
// expected-error: S016 at line 8, the export specifier
// tsc-error-count: 1
export { nope } from "./lib";
export function main(): void {}
