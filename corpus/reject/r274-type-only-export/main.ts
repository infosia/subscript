// corpus: reject/r274-type-only-export/main
// purpose: Rejects a type-only re-export list with the C18 diagnostic.
// exercises: module-export
// questions: compiler.md §128, C18
// tsc: accepts
// expected-error: S100 at line 8, the export form (C18)

export type { Label, Box } from "./lib";
export function main(): void {}
