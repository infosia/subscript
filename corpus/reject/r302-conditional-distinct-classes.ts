// corpus: reject/r302-conditional-distinct-classes
// purpose: A conditional cannot join two different classes.
// exercises: conditional-expression, branch-join
// questions: compiler.md §146, collisions.md C7
// tsc: accepts
// expected-error: S100 at the conditional with incompatible branch types
class A { value: i32 = 1; }
class B { value: i32 = 2; }
export function main(): void { const value = true ? new A() : new B(); }
