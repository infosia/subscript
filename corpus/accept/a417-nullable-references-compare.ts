// corpus: accept/a417-nullable-references-compare
// purpose: Two nullable references of one type compare by identity with each equality operator.
// observable: each form prints both absent, one absent, the same object, and two objects.
// exercises: nullable-reference, equality, undefined, map-get, linked-structure
// questions: compiler.md §228, §219.1 rule 4a, §195
// tsc: accepts; js-comparable: yes
class C {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class N {
  next: N | null = null;
}

function strictEq(a: C | null, b: C | null): boolean {
  return a === b;
}

function strictNe(a: C | null, b: C | null): boolean {
  return a !== b;
}

function looseEq(a: C | null, b: C | null): boolean {
  return a == b;
}

function looseNe(a: C | null, b: C | null): boolean {
  return a != b;
}

function cases(f: (a: C | null, b: C | null) => boolean): string {
  const x: C = new C(1);
  const y: C = new C(1);
  return `${f(null, null)}:${f(x, null)}:${f(null, x)}:${f(x, x)}:${f(x, y)}`;
}

function nextEq(a: N, b: N): boolean {
  return a.next === b.next;
}

function linked(): string {
  const a: N = new N();
  const b: N = new N();
  const both: boolean = nextEq(a, b);
  const shared: N = new N();
  a.next = shared;
  const one: boolean = nextEq(a, b);
  b.next = shared;
  const same: boolean = nextEq(a, b);
  b.next = new N();
  const two: boolean = nextEq(a, b);
  return `${both}:${one}:${same}:${two}`;
}

function mapGet(m: Map<i32, C>): boolean {
  return m.get(1) === m.get(2);
}

function mapGetNe(m: Map<i32, C>): boolean {
  return m.get(1) !== m.get(2);
}

function mapCases(f: (m: Map<i32, C>) => boolean): string {
  const both: Map<i32, C> = new Map<i32, C>();
  const one: Map<i32, C> = new Map<i32, C>();
  const x: C = new C(1);
  one.set(1, x);
  const same: Map<i32, C> = new Map<i32, C>();
  same.set(1, x);
  same.set(2, x);
  const two: Map<i32, C> = new Map<i32, C>();
  two.set(1, x);
  two.set(2, new C(1));
  return `${f(both)}:${f(one)}:${f(same)}:${f(two)}`;
}

function mapsEq(a: Map<i32, i32> | null, b: Map<i32, i32> | null): boolean {
  return a === b;
}

function mapsLooseNe(a: Map<i32, i32> | null, b: Map<i32, i32> | null): boolean {
  return a != b;
}

function nullableMaps(f: (a: Map<i32, i32> | null, b: Map<i32, i32> | null) => boolean): string {
  const x: Map<i32, i32> = new Map<i32, i32>();
  const y: Map<i32, i32> = new Map<i32, i32>();
  return `${f(null, null)}:${f(x, null)}:${f(x, x)}:${f(x, y)}`;
}

function pick(m: Map<i32, C>, k: i32): C | undefined {
  return m.get(k);
}

function acrossEq(a: C | null, m: Map<i32, C>, k: i32): boolean {
  return a == pick(m, k);
}

function acrossNe(a: C | null, m: Map<i32, C>, k: i32): boolean {
  return pick(m, k) != a;
}

function across(f: (a: C | null, m: Map<i32, C>, k: i32) => boolean): string {
  const x: C = new C(1);
  const m: Map<i32, C> = new Map<i32, C>();
  m.set(1, x);
  m.set(2, new C(1));
  return `${f(null, m, 0)}:${f(x, m, 0)}:${f(null, m, 1)}:${f(x, m, 1)}:${f(x, m, 2)}`;
}

export function main(): void {
  print(`strictEq:${cases(strictEq)}`);
  print(`strictNe:${cases(strictNe)}`);
  print(`looseEq:${cases(looseEq)}`);
  print(`looseNe:${cases(looseNe)}`);
  print(`linked:${linked()}`);
  print(`mapGet:${mapCases(mapGet)}`);
  print(`mapGetNe:${mapCases(mapGetNe)}`);
  print(`mapsEq:${nullableMaps(mapsEq)}`);
  print(`mapsLooseNe:${nullableMaps(mapsLooseNe)}`);
  print(`acrossEq:${across(acrossEq)}`);
  print(`acrossNe:${across(acrossNe)}`);
}
