// corpus: reject/r321-namespace-export/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace export form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 8, namespace form
export * as ns from "./lib";
export function main(): void {}
