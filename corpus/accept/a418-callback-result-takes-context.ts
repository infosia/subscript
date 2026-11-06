// corpus: accept/a418-callback-result-takes-context
// purpose: The body of a callback whose result is a type parameter takes the context of the call.
// observable: each form prints the values that its callback gives under the context type.
// exercises: generic-inference, contextual-type, lambda, array-map, array-flatmap, string-literal-union, generic-method, generic-class, numeric-context
// questions: compiler.md §229, §212
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";

class P {
  v: i32 = 7;
}

class Buf<T> {
  items: T[] = [];
  size(): i32 {
    return this.items.length;
  }
}

function empty<T>(): Buf<T> {
  return new Buf<T>();
}

function ap<T, U>(x: T, f: (v: T) => U): U {
  return f(x);
}

class Util {
  static ap<T, U>(x: T, f: (v: T) => U): U {
    return f(x);
  }

  twice<T, U>(x: T, f: (v: T) => U): U[] {
    return [f(x), f(x)];
  }
}

class Cell<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }

  apply(f: (v: T) => T): Cell<T> {
    return new Cell<T>(f(this.value));
  }
}

function flip(ms: Mode[]): Mode[] {
  return ms.map((x: Mode) => x === "a" ? "b" : "a");
}

function flipExplicit(ms: Mode[]): Mode[] {
  return ms.map<Mode>((x: Mode) => x === "a" ? "b" : "a");
}

function constant(xs: i32[]): Mode[] {
  return xs.map((x: i32) => "a");
}

function buffers(xs: i32[]): Buf<P>[] {
  return xs.map((x: i32) => empty());
}

function flat(xs: i32[]): Mode[] {
  return xs.flatMap((x: i32) => ["a"]);
}

function applied(n: i32): Mode {
  return ap(n, (v: i32) => "a");
}

function appliedExplicit(n: i32): Mode {
  return ap<i32, Mode>(n, (v: i32) => "a");
}

function nested(n: i32): Mode {
  return ap(n, (v: i32) => ap(v, (w: i32) => w > 0 ? "b" : "a"));
}

function nestedBuffer(n: i32): Buf<P> {
  return ap(n, (v: i32) => empty());
}

function staticMethod(n: i32): Mode {
  return Util.ap(n, (v: i32) => "b");
}

function instanceMethod(n: i32): Mode[] {
  return new Util().twice(n, (v: i32) => "a");
}

function cell(m: Mode): Mode {
  return new Cell<Mode>(m).apply((v: Mode) => v === "a" ? "b" : "a").value;
}

function nullableContext(xs: i32[]): Mode[] | null {
  return xs.map((x: i32) => "b");
}

export function main(): void {
  print(`flip:${flip(["a", "b", "a"]).join(",")}`);
  print(`flipExplicit:${flipExplicit(["b", "a"]).join(",")}`);
  print(`constant:${constant([1, 2]).join(",")}`);
  const bs: Buf<P>[] = buffers([1, 2, 3]);
  print(`buffers:${bs.length}:${bs[0].size()}`);
  print(`flat:${flat([1, 2]).join(",")}`);
  print(`applied:${applied(1)}:${appliedExplicit(2)}`);
  print(`nested:${nested(1)}:${nested(0)}:${nestedBuffer(3).size()}`);
  print(`methods:${staticMethod(1)}:${instanceMethod(1).join(",")}`);
  print(`cell:${cell("a")}:${cell("b")}`);
  const xs: i32[] = [1, 2];
  const l: i64[] = xs.map((x: i32) => 2147483647);
  print(`i64:${l[0] + 1}`);
  const r: f64[] = xs.map((x: i32) => 1);
  print(`f64:${r[0] / 4}`);
  const f: f64[] = xs.flatMap((x: i32) => [1, 2]);
  print(`flatF64:${f.length}:${f[1] / 4}`);
  const d: f64 = ap(3, (v: i32) => ap(v, (w: i32) => 1));
  print(`nestedF64:${d / 4}`);
  const e = xs.map<i64>((x: i32) => 2147483647);
  print(`explicitI64:${e[0] + 1}`);
  const n: Mode[] | null = nullableContext([1]);
  if (n !== null) {
    print(`nullable:${n.join(",")}`);
  }
}
