// corpus: reject/r316-namespace-argument/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace argument form.
// tsc: rejects TS2345
// divergence: C18
// expected-error: S100 at line 10, namespace form
import * as ns from "./lib";
function take(x: i32): void {}
export function main(): void { take(ns); }
