// corpus: reject/r317-namespace-stored/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace stored form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { const value = ns; }
