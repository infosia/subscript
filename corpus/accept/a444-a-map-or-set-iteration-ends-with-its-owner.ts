// corpus: accept/a444-a-map-or-set-iteration-ends-with-its-owner
// purpose: A Map or Set visit ends with its cursor owner.
// observable: Scope exits release visits; visits skip removed entries and reach added entries.
// exercises: map, set, for-of, break, return, exception, generator
// questions: compiler.md §271
// tsc: accepts; js-comparable: yes
function leave(m: Map<i32, i32>): void {
  for (const v of m.values()) { print(`return:${v}`); return; }
}
function* visit(m: Map<i32, i32>): Generator<i32> {
  for (const v of m.values()) { yield v; }
}
function drop(m: Map<i32, i32>): void {
  const g = visit(m);
  print(`generator:${g.next().value}`);
}
export async function main(): Promise<void> {
  const m = new Map<i32, i32>();
  m.set(1, 10); m.set(2, 20); m.set(3, 30);
  for (const v of m.values()) { print(`break:${v}`); break; }
  leave(m);
  try {
    for (const v of m.values()) { print(`throw:${v}`); throw new Error("exit"); }
  } catch (e) { print("caught"); }
  drop(m);
  for (const k of m.keys()) {
    if (k === 1) { m.delete(2); }
    print(`delete:${k}`);
  }
  for (const k of m.keys()) {
    if (k === 1) { m.set(4, 40); }
    print(`insert:${k}`);
  }
  const s = new Set<i32>(); s.add(5); s.add(6);
  for (const v of s) { print(`set:${v}`); }
}
