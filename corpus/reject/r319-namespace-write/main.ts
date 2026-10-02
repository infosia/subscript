// corpus: reject/r319-namespace-write/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace write form.
// tsc: rejects TS2540
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export function main(): void { ns.count = 1; }
