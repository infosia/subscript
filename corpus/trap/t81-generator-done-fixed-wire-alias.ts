// corpus: trap/t81-generator-done-fixed-wire-alias
// purpose: A finished wire-alias generator traps when wire zero has no member.
// exercises: generator, iterator-result, CEnum, fixed-array
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// expected-trap: generator-done-value at r.value (15:15)
type W = CEnum<{ w3: 3; w5: 5; }>;
function* values(): Generator<FixedArray<W, 2>> { yield ["w5", "w3"]; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  const v = r.value;
  print(`v=${v[0]}|${v[0] == "w3"} ${v[1] == "w3"}`);
}
