// corpus: accept/a321-namespace-cycle/x
// purpose: Namespace imports preserve dependency initialization order in a module cycle.
// exercises: namespace-import, module-cycle, module-initialization
// questions: compiler.md §148, compiler.md §137
// tsc: accepts
import * as ns from "./y";
export const x: i32 = ns.y + 1;
print("x");
