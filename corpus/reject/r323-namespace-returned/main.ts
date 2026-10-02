// corpus: reject/r323-namespace-returned/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace returned form.
// tsc: rejects TS2322
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
function get(): i32 { return ns; }
export function main(): void {}
