// corpus: reject/r485-callback-result-wider-than-context
// purpose: A callback body result that is not a literal decides its type parameter before the context, so a wider result stays rejected.
// exercises: generic-inference, contextual-type, lambda, array-map, string-literal-union
// questions: compiler.md §229.1 rule 2, §212.1 rule 2
// tsc: rejects TS2322
// expected-error: S100 at line 12, the return value expects `Mode[]`, got `string[]`

type Mode = "a" | "b";

function modes(xs: i32[]): Mode[] {
  const s: string = "a";
  return xs.map((x: i32) => s);
}

export function main(): void {
  print(`${modes([1]).length}`);
}
