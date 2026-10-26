// corpus: reject/r437-undefined-conditional-cross-join
// purpose: A conditional whose branches have the two absence spellings is rejected; it needs both spellings.
// exercises: undefined, conditional, nullable
// questions: compiler.md §195.1 rule 3
// tsc: accepts
// expected-error: S011, `T | null | undefined` has no representation
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}
export function main(): void {
  const u = pick();
  const n: Box | null = null;
  const c = u === undefined;
  const x = c ? u : n;
  print(`${x === null}`);
}
