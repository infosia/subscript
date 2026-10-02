// corpus: accept/a321-namespace-cycle/y
// purpose: Namespace imports preserve dependency initialization order in a module cycle.
// exercises: namespace-import, module-cycle, module-initialization
// questions: compiler.md §148, compiler.md §137
// tsc: accepts
import * as ns from "./x";
export const y: i32 = 2;
print("y");
