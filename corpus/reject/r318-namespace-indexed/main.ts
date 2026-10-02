// corpus: reject/r318-namespace-indexed/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace indexed form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { const k = "count"; print(`${ns[k]}`); }
