// corpus: reject/r421-text-module-unknown-export
// purpose: An import of a name that subscript:text does not export is rejected.
// exercises: text-module, import
// questions: compiler.md §193 rule 1, compiler.md §193 rule 7
// tsc: rejects TS2305
// expected-error: S100, subscript:text has no export `graphemeWidth`
import { graphemeWidth } from "subscript:text";
export function main(): void {
  print(`${graphemeWidth("が")}`);
}
