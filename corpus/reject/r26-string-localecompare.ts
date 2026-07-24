// corpus: reject/r26-string-localecompare
// purpose: Rejects `localeCompare`: locale-dependent collation is out of
//          the accepted subset (Q21 is ASCII/byte-based).
// exercises: rejected-string-subset, string-methods
// questions: Q21
// expected-error: localeCompare is locale-dependent; out of subset

export function main(): void {
  const s: string = "a";
  const r: i32 = s.localeCompare("b");
  print(`${r}`);
}
