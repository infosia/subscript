// corpus: reject/r436-undefined-conditional-null-join
// purpose: A conditional that joins the null literal with an undefined-spelled value is rejected; it needs both spellings.
// exercises: undefined, conditional, nullable
// questions: compiler.md §195.1 rule 3, compiler.md §195.10
// tsc: accepts
// expected-error: S011, `T | null | undefined` has no representation
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  const c = pick() === undefined;
  const x = c ? null : pick();
  print(`${x == null}`);
}
