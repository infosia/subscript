// corpus: reject/r501-an-alias-array-is-not-a-string-array
// purpose: Alias arrays keep their discriminant storage.
// exercises: string-alias, array
// questions: compiler.md §259, collisions.md Q32
// tsc: accepts
// expected-error: S100 at line 10

type Mode = "a" | "b";
function f(ms: Mode[]): void {
  const xs: string[] = ms;
}
