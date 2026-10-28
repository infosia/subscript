// corpus: reject/r446-type-alias-refers-to-itself
// purpose: An alias that reaches itself names no finite type.
// exercises: type-alias
// questions: compiler.md §202.1 rule 4
// tsc: accepts
// expected-error: S100, type alias `Tree` refers to itself
type Tree = Tree[];

export function main(): void {
  const t: Tree = [];
  print(`${t.length}`);
}
