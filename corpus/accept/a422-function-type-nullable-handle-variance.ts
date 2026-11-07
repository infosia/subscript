// corpus: accept/a422-function-type-nullable-handle-variance
// purpose: A function value fits a function type whose result adds null to a handle type, or whose parameter drops it.
// observable: each call through a converted function value prints the value of the original function.
// exercises: function-value, nullable-reference, lambda, array-reduce, class-field
// questions: compiler.md §235, collisions.md C24
// tsc: accepts; js-comparable: yes
class C {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Holder {
  f: (x: C) => C | null;
  constructor(f: (x: C) => C | null) {
    this.f = f;
  }
}

function show(c: C | null): string {
  return c === null ? "null" : `${c.v}`;
}

function applyResult(f: (x: C) => C | null, c: C): string {
  return show(f(c));
}

function applyParameter(f: (x: C) => i32, c: C): i32 {
  return f(c);
}

function text(f: () => string | null): string {
  const s: string | null = f();
  return s === null ? "null" : s;
}

function size(f: () => i32[] | null): i32 {
  const xs: i32[] | null = f();
  return xs === null ? -1 : xs.length;
}

function entries(f: () => Map<i32, i32> | null): i32 {
  const m: Map<i32, i32> | null = f();
  return m === null ? -1 : m.size;
}

function make(): (x: C) => C | null {
  return (x: C): C => x;
}

function id(x: C): C {
  return x;
}

function pick(x: C | null): boolean {
  return x !== null && x.v > 5;
}

function seven(): i32 {
  return 7;
}

export function main(): void {
  const c: C = new C(4);
  print(`result:${applyResult((x: C): C => x, c)}`);
  print(`parameter:${applyParameter((x: C | null): i32 => (x === null ? 0 : x.v + 1), c)}`);
  const g: (x: C) => C | null = (x: C): C => x;
  print(`initializer:${show(g(new C(5)))}`);
  const last: C | null = [new C(6), new C(7)].reduce((acc: C | null, x: C): C => x, null);
  print(`reduce:${show(last)}`);
  print(`string:${text((): string => "a")}`);
  print(`array:${size((): i32[] => [1, 2])}`);
  print(`map:${entries((): Map<i32, i32> => new Map<i32, i32>())}`);
  const h: Holder = new Holder((x: C): C => x);
  print(`field:${show(h.f(new C(8)))}`);
  print(`return:${show(make()(new C(9)))}`);
  const nullableResult: () => (() => i32) | null = (): (() => i32) => seven;
  const inner: (() => i32) | null = nullableResult();
  print(`nullableFunction:${inner === null ? -1 : inner()}`);
  const cs: C[] = [new C(3), new C(6)];
  const mapped: i32[] = cs.map((x: C | null): i32 => (x === null ? -1 : x.v));
  print(`arrayMap:${mapped.join(",")}`);
  const filtered: C[] = cs.filter((x: C | null): boolean => x !== null && x.v > 4);
  print(`filter:${filtered.length}:${filtered[0].v}`);
  print(`findIndex:${cs.findIndex(pick)}`);
  const total: i32 = cs.reduce((acc: i32, x: C | null): i32 => acc + (x === null ? 0 : x.v), 0);
  print(`reduceElement:${total}`);
  const byKey: Map<i32, C> = new Map<i32, C>();
  byKey.set(1, new C(11));
  const seen: C = new C(0);
  byKey.forEach((value: C | null, key: i32): void => {
    seen.v = seen.v + key + (value === null ? 0 : value.v);
  });
  print(`mapForEach:${seen.v}`);
  const fs: ((x: C) => C | null)[] = [];
  fs.push(id);
  print(`push:${show(fs[0](new C(12)))}`);
  const table: Map<i32, (x: C) => C | null> = new Map<i32, (x: C) => C | null>();
  table.set(1, id);
  const stored = table.get(1);
  print(`mapSet:${stored === undefined ? "none" : show(stored(new C(13)))}`);
  const none: ((x: C) => C | null) | null = null;
  const w: (x: C) => C | null = none ?? id;
  print(`coalesce:${show(w(new C(11)))}`);
}
