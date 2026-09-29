// corpus: reject/r278-entry-signature/main
// purpose: Rejects a non-void host entry.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts
// collision: C18
// expected-error: S100 at line 8
export function read(): i32 { return 1; }
export function main(): void {}
