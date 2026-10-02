// corpus: reject/r320-namespace-missing/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace missing form.
// tsc: rejects TS2339
// divergence: C18
// expected-error: S016 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { ns.nope(); }
