// corpus: reject/r328-namespace-local-export/main
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Rejects a namespace binding in a local export list.
// tsc: accepts
// divergence: C18
// expected-error: S100 at line 9, namespace form
import * as ns from "./lib";
export { ns };
export function main(): void {}
