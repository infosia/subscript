// corpus: accept/a268-array-string-at-find-flatmap
// purpose: Signed indexing, reference searches, reverse predicates, and depth-one mapping (stdlib.md §8.10, §9.10).
// exercises: array, string, callback
// questions: Q21, Q22
// tsc: accepts; js-comparable: yes
class Cell {
  v: i32;
  constructor(v: i32) { this.v = v; }
}
function expand(v: i32, i: i32): i32[] { return [v, i]; }
function indirect(f: (v: i32, i: i32) => i32[]): i32[] {
  return [4, 5].flatMap(f);
}
export function main(): void {
  const a: i32[] = [1, 2, 3];
  print(`${a.at(0)} ${a.at(-1)} ${a.at(-3)}`);
  print(`${"abc".at(0)} ${"abc".at(-1)} ${"héllo".at(1)}`);
  const cells: Cell[] = [new Cell(1), new Cell(2), new Cell(1)];
  print(`${cells.findIndex((c: Cell): boolean => c.v === 1)} ${cells.findLastIndex((c: Cell): boolean => c.v === 1)} ${cells.findLastIndex((c: Cell): boolean => c.v === 7)}`);
  print(`${(cells.find((c: Cell): boolean => c.v === 1) ?? new Cell(-1)).v} ${(cells.findLast((c: Cell): boolean => c.v === 1) ?? new Cell(-1)).v} ${(cells.find((c: Cell): boolean => c.v === 7) ?? new Cell(-1)).v}`);
  print(a.flatMap((v: i32): i32[] => [v, v * 10]).join(","));
  print(`[${[1, 2].flatMap((v: i32): i32[] => []).join(",")}]`);
  const nested: i32[][] = [[1], [2]];
  print(nested.flatMap((v: i32[]): i32[] => v).join(","));
  print([4, 5].flatMap((v: i32, i: i32): i32[] => [i]).join(","));
  print(indirect(expand).join(","));
  const offset: i32 = 10;
  print(a.flatMap((v: i32): i32[] => [v + offset]).join(","));
  print(`${cells.findLastIndex((c: Cell, i: i32): boolean => i < 2)} ${(cells.findLast((c: Cell, i: i32): boolean => i < 2) ?? new Cell(-1)).v}`);
  const empty: Cell[] = [];
  print(`${empty.findLastIndex((c: Cell): boolean => true)} ${(empty.findLast((c: Cell): boolean => true) ?? new Cell(-1)).v}`);
}
