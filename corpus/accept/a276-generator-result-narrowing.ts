// corpus: accept/a276-generator-result-narrowing
// purpose: A nullable generator result field narrows as a local value.
// exercises: generator, nullable-field, local-narrowing
// questions: compiler.md §124
// tsc: accepts; js-comparable: yes
class Cell { v: i32 = 7; }
function* cells(): Generator<Cell | null> {
  yield new Cell();
  yield null;
}
export function main(): void {
  const g = cells();
  const r = g.next();
  if (r.value !== null) { print(`value=${r.value.v}`); }
  const r2 = g.next();
  if (r2.value === null) { print("null2"); }
}
