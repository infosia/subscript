// corpus: reject/r487-unread-type-argument
// purpose: A type argument list that the checker does not read is rejected, and the diagnostic names the count that the method takes.
// exercises: array-filter, type-arguments
// questions: compiler.md §230.1 rule 3
// tsc: rejects TS2345
// expected-error: S100 at line 9, `filter` takes no type arguments, got 1

function positive(xs: i32[]): i32[] {
  return xs.filter<i32>((x: i32) => x > 0);
}

export function main(): void {
  print(`${positive([1, -2, 3]).length}`);
}
