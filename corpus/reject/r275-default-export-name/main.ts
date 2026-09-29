// corpus: reject/r275-default-export-name/main
// purpose: Rejects the default export name in a named re-export.
// exercises: module-export
// questions: compiler.md §128, C18
// tsc: accepts
// expected-error: S100 at line 8, the export form (C18)

export { value as default } from "./lib";
export function main(): void {}
