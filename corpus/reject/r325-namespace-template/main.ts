// corpus: reject/r325-namespace-template/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace template form.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { print(`${ns}`); }
