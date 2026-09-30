// corpus: accept/a299-module-post-order/main
// purpose: Reads an imported global after dependencies initialize.
// exercises: module-import, module-initializer
// questions: compiler.md §137, C18
// tsc: accepts; js-comparable: yes

import { vb } from "./b";
import { vc } from "./c";
const result: i32 = vb + vc;
print("main");
export function main(): void { print(`${result}`); }
