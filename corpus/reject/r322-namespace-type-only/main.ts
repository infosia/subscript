// corpus: reject/r322-namespace-type-only/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace type-only form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 8, namespace form
import type * as ns from "./lib";
export function main(): void {}
