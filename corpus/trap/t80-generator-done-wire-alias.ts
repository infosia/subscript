// corpus: trap/t80-generator-done-wire-alias
// purpose: A finished wire-alias generator traps when wire zero has no member.
// exercises: generator, iterator-result, CEnum, switch
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// expected-trap: generator-done-value at r.value (21:18)
type W = CEnum<{ w3: 3; w5: 5; }>;
function tag(v: W): i32 {
  switch (v) {
    case "w3": return 3;
    case "w5": return 5;
  }
}
function* values(): Generator<W> { yield "w5"; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  const v: W = r.value;
  print(`done=${r.done}|tag=${tag(v)}|v=${v}|end`);
}
