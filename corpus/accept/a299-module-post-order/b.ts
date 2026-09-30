// corpus: accept/a299-module-post-order/b
// purpose: Reads a dependency during global initialization.
// exercises: module-import, module-global
// questions: compiler.md §137, C18
// tsc: accepts; js-comparable: yes

import { va } from "./a";
export const vb: i32 = va + 1;
print("b");
