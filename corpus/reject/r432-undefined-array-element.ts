// corpus: reject/r432-undefined-array-element
// purpose: An array literal whose element type has the undefined spelling is rejected.
// exercises: undefined, array
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, the `undefined` spelling is legal only as the whole type of a value
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  const xs = [pick()];
  print(`${xs.length}`);
}
