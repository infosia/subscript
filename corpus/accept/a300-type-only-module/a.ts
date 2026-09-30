// corpus: accept/a300-type-only-module/a
// purpose: Defines a type whose module initializer runs.
// exercises: module-import
// questions: compiler.md §137, C18
// tsc: accepts; js-comparable: no C18: Type-only imports run module initializers.

export class A { value: i32 = 2; }
print("a init");
