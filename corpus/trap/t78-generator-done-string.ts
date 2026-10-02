// corpus: trap/t78-generator-done-string
// purpose: A finished generator value must trap at the read.
// exercises: generator, iterator-result, string
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// expected-trap: generator-done-value at r.value (14:23)
function* values(): Generator<string> { yield "live"; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  const s: string = r.value;
  print(`${s == ""}`);
  print(`${s.length}`);
  print("end");
}
