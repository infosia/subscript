// corpus: reject/r300-generator-result-pattern-field
// purpose: An iterator result cannot supply a binding pattern.
// exercises: generator, iterator-result, binding-pattern
// questions: compiler.md §145 rule 1a, compiler.md §107.1
// tsc: rejects TS2339
// tsc-message: Property 'other' does not exist on type 'IteratorResult<number, any>'.
// expected-error: S100: an iterator result cannot supply a binding pattern; use `const r = it.next(); if (r.done) ...`
function* values(): Generator<i32> { yield 7; }
export function main(): void {
  const iterator = values();
  const { other } = iterator.next();
  print(`${other}`);
}
