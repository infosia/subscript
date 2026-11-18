// corpus: reject/r556-generic-class-external-member-constraint
// purpose: An external generic-class field read has its plain instance type.
// exercises: interface, generic-class, nullable-reference
// questions: compiler.md §335 rule 1a
// tsc: rejects TS2322
// expected-error: S100 at line 14, the assignment expects `C`, got `null`
class C { v: i32 = 7; }
interface R { m(): C | null; }
class K implements R { m(): C { return new C(); } }
class B<T extends R> { t: T; constructor(t: T) { this.t = t; } }
export function main(): void {
  const b = new B(new K());
  let o = b.t.m();
  o = null;
}
