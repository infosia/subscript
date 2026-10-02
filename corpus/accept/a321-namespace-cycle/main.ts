// corpus: accept/a321-namespace-cycle/main
// purpose: Namespace imports preserve dependency initialization order in a module cycle.
// exercises: namespace-import, module-cycle, module-initialization
// questions: compiler.md §148, compiler.md §137
// tsc: accepts; js-comparable: yes
import * as ns from "./x";
print("main");
export function main(): void { print(`${ns.x}`); }
