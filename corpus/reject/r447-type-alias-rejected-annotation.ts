// corpus: reject/r447-type-alias-rejected-annotation
// purpose: An aliased type is checked as an annotation is, and the alias reports the annotation's text.
// exercises: type-alias, union-types
// questions: compiler.md §202.1 rule 2
// tsc: accepts
// expected-error: S011, unions are limited to `Ref | null`
type Num = i32 | f64;

export function main(): void {
  const n: Num = 1;
  print(`${n}`);
}
