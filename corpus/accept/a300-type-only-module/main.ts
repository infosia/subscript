// corpus: accept/a300-type-only-module/main
// purpose: Runs the initializer of a type-only dependency first.
// exercises: module-import, module-initializer
// questions: compiler.md §137, C18
// tsc: accepts; js-comparable: no C18: Type-only imports run module initializers.

import type { A } from "./a";
import { vb } from "./b";
function take(value: A): void { print(`${value.value}`); }
print("entry");
export function main(): void { print(`${vb}`); }
