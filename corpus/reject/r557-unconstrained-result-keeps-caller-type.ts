// corpus: reject/r557-unconstrained-result-keeps-caller-type
// purpose: An unconstrained result keeps each caller's own argument type.
// exercises: interface, generic-function, nullable-reference, generic-result
// questions: compiler.md §335 rule 1b
// tsc: rejects TS2322
// expected-error: S100 at line 18, the assignment expects `C`, got `null`
class C { v: i32 = 7; }
interface R { m(): C | null; }
class K implements R { m(): C { return new C(); } }
function pick<T>(x: T): T { return x; }
function viaPick<U extends R>(x: U): i32 {
  const o = pick(x).m(); return o === null ? -1 : o.v;
}
export function main(): void {
  const k = new K();
  viaPick(k);
  let o = pick(k).m();
  o = null;
}
