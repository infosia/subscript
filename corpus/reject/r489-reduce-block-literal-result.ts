// corpus: reject/r489-reduce-block-literal-result
// purpose: A string literal that a block-body reduce callback returns widens to string under the element type, so it does not meet a string literal accumulator.
// exercises: array-reduce, string-literal-union, lambda
// questions: compiler.md §231.1 rule 3, §230.1 rule 1
// tsc: rejects TS2322, TS2769
// expected-error: S100 at line 12, `tsc` rejects the call (TS2769)

type Mode = "a" | "b";

function last(xs: i32[]): Mode {
  return xs.reduce((acc: Mode, x: i32) => {
    return "b";
  }, "a");
}

export function main(): void {
  print(`${last([1, 2])}`);
}
