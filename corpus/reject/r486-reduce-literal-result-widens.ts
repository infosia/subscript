// corpus: reject/r486-reduce-literal-result-widens
// purpose: A string literal result of a reduce callback widens to string under the element type, so it does not meet a string literal accumulator.
// exercises: array-reduce, string-literal-union, lambda
// questions: compiler.md §230.1 rule 1
// tsc: rejects TS2322, TS2769
// expected-error: S100 at line 11, `tsc` rejects the call (TS2769)

type Mode = "a" | "b";

function last(xs: i32[]): Mode {
  return xs.reduce((acc: Mode, x: i32) => "b", "a");
}

export function main(): void {
  print(`${last([1, 2])}`);
}
