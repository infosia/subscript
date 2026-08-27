// corpus: reject/r53-set-algebra-nonset
// purpose: Rejects JS's set-like duck-typed algebra argument because
//          the language has no set-like protocol.
// exercises: Set-algebra, set-like-protocol, rejected-structural-argument
// expected-error: S014 at the non-Set argument
// questions: Q27
// tsc: rejects TS2345
export function main(): void {
  const values: Set<i32> = new Set<i32>();
  values.union([1, 2]);
}
