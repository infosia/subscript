// corpus: reject/r326-namespace-typeof/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace typeof form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { print(typeof ns); }
