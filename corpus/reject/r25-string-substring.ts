// corpus: reject/r25-string-substring
// purpose: Rejects `substring`: redundant with the byte-measure `slice`,
//          which is the one accepted slicing surface (Q21).
// exercises: rejected-string-subset, string-methods
// questions: Q21
// expected-error: substring is out of subset; use slice

export function main(): void {
  const s: string = "hello";
  const t: string = s.substring(1, 3);
  print(t);
}
