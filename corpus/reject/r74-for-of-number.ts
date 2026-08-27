// corpus: reject/r74-for-of-number
// purpose: Rejects for-of over a sized number.
// exercises: for-of-closed-list
// questions: Q30
// tsc: rejects TS2488
// expected-error: i32 is not an iterable container
export function main(): void {
  const source: i32 = 3;
  for (const value of source) {
    print(`${value}`);
  }
}
