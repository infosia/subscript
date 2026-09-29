// corpus: reject/r277-entry-nonfunction/main
// purpose: Rejects a class in the entry API.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts
// collision: C18
// expected-error: S100 at line 8
export class State { value: i32 = 0; }
export function main(): void {}
