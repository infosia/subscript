// corpus: reject/r558-external-generic-member-callback
// purpose: An external method callback receives the plain class argument type.
// exercises: interface, generic-class, contextual-lambda, nullable-reference
// questions: compiler.md §335 rule 1a
// tsc: rejects TS2322
// expected-error: S100 at line 17, the assignment expects `C`, got `null`
class C { v: i32 = 7; }
interface R { m(): C | null; }
class K implements R { m(): C { return new C(); } }
class B<T extends R> {
  t: T;
  constructor(t: T) { this.t = t; }
  each(f: (t: T) => i32): i32 { return f(this.t); }
}
export function main(): void {
  const b = new B(new K());
  print(`${b.each((t) => { let o = t.m(); o = null; return 1; })}`);
}
