// corpus: accept/a19-modules/main
// purpose: Imports and calls a function from a sibling module.
// exercises: module-import, module-export, entry-point
// questions: Q1, Q12

import { triangular } from "./math";

export function main(): void {
  print(`${triangular(10)}`);
}
