// corpus: reject/r221-pattern-source-shape
// purpose: Rejects an array binding pattern over a string source.
// exercises: binding-pattern, source-shape, string
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the array source an array pattern reads
export function main(): void {
  const text: string = "ab";
  const [first, second] = text;
  print(`${first} ${second}`);
}
