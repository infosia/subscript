// corpus: reject/r438-undefined-to-null-conversion
// purpose: A value of the undefined spelling does not convert to the null spelling.
// exercises: undefined, nullable, assignment
// questions: compiler.md §195.1 rule 2
// tsc: rejects TS2322
// expected-error: S012, `T | undefined` and `T | null` do not convert to each other
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}
export function main(): void {
  const u = pick();
  const b: Box | null = u;
  print(`${b === null}`);
}
