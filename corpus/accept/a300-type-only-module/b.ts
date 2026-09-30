// corpus: accept/a300-type-only-module/b
// purpose: Defines a runtime dependency.
// exercises: module-global
// questions: compiler.md §137, C18
// tsc: accepts; js-comparable: no C18: Type-only imports run module initializers.

export const vb: i32 = 2;
print("b init");
