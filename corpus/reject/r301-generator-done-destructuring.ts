// corpus: reject/r301-generator-done-destructuring
// purpose: An iterator result cannot supply a binding pattern.
// exercises: generator, iterator-result, binding-pattern
// questions: compiler.md §145, compiler.md §107
// tsc: accepts
// expected-error: S100: an iterator result cannot supply a binding pattern; use `const r = it.next(); if (r.done) ...`
function* values(): Generator<string> { yield "a"; yield "b"; }
export function main(): void {
  const it = values();
  while (true) {
    const { done, value } = it.next();
    if (done) { break; }
    print(value);
  }
  print("end");
}
