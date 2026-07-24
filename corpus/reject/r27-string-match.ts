// corpus: reject/r27-string-match
// purpose: Rejects `match`: it requires RegExp, which is a stdlib
//          non-goal (Q21; stdlib.md §7).
// exercises: rejected-string-subset, string-methods
// questions: Q21
// expected-error: match requires RegExp; out of subset

export function main(): void {
  const s: string = "hello";
  const m: string = s.match("l");
  print(m);
}
