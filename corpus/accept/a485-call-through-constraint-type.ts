// corpus: accept/a485-call-through-constraint-type
// purpose: Calls through type parameters keep their declared constraint method types.
// observable: Each constrained receiver form prints its result.
// exercises: interface, generic-function, generic-class, nullable-reference, function-value, Map, for-of, array-index, generic-result
// questions: compiler.md §335
// tsc: accepts; js-comparable: yes
class C { v: i32 = 7; }
interface R {
  m(): C | null;
  f(): (() => i32) | null;
  a(): i32[] | null;
  map(): Map<i32, i32> | null;
}
function seven(): i32 { return 7; }
class K implements R {
  m(): C { return new C(); }
  f(): () => i32 { return seven; }
  a(): i32[] { return [7]; }
  map(): Map<i32, i32> { const m = new Map<i32, i32>(); m.set(1, 7); return m; }
}
class K0 implements R {
  m(): C | null { return new C(); }
  f(): (() => i32) | null { return seven; }
  a(): i32[] | null { return [7]; }
  map(): Map<i32, i32> | null { const m = new Map<i32, i32>(); m.set(1, 7); return m; }
}
function g<U extends R>(x: U): i32 {
  const o = x.m();
  if (o !== null) { return o.v; }
  return -1;
}
function h<U extends R>(x: U): i32 {
  let fn = x.f();
  if (fn === null) { return -1; }
  const result = fn();
  fn = null;
  return result;
}
function arrays<U extends R>(x: U): i32 {
  const a = x.a();
  if (a === null) { return -1; }
  return a[0];
}
function maps<U extends R>(x: U): i32 {
  const m = x.map();
  if (m === null) { return -1; }
  return m.size;
}
function both<U extends R, V extends R>(x: U, y: V): i32 {
  const a = x.m();
  const b = y.m();
  if (a !== null && b !== null) { return a.v + b.v; }
  return -1;
}
class B<T extends R> {
  t: T;
  constructor(t: T) { this.t = t; }
  run(): i32 {
    let o = this.t.m();
    if (o === null) { return -1; }
    o = null;
    return 0;
  }
}
function loop<U extends R>(xs: U[]): i32 {
  let result: i32 = 0;
  for (const x of xs) {
    let o = x.m();
    if (o === null) { return -1; }
    result += o.v;
    o = null;
  }
  return result;
}
function element<U extends R>(xs: U[]): i32 {
  let o = xs[0].m();
  if (o === null) { return -1; }
  const result = o.v;
  o = null;
  return result;
}
function narrowed<U extends R>(x: U | null): i32 {
  if (x === null) { return -1; }
  let o = x.m();
  if (o === null) { return -1; }
  const result = o.v;
  o = null;
  return result;
}
function id<U extends R>(x: U): U { return x; }
function returned<U extends R>(x: U): i32 {
  let o = id(x).m();
  if (o === null) { return -1; }
  const result = o.v;
  o = null;
  return result;
}
function first<T>(xs: T[]): T { return xs[0]; }
function firstMain<T>(xs: T[]): T { return xs[0]; }
function pick<T>(x: T): T { return x; }
function pickMain<T>(x: T): T { return x; }
function viaFirst<U extends R>(xs: U[]): i32 {
  const o = first(xs).m(); return o === null ? -1 : o.v;
}
function viaFirstMain<U extends R>(xs: U[]): i32 {
  const o = firstMain(xs).m(); return o === null ? -1 : o.v;
}
function viaPick<U extends R>(x: U): i32 {
  const o = pick(x).m(); return o === null ? -1 : o.v;
}
function viaPickMain<U extends R>(x: U): i32 {
  const o = pickMain(x).m(); return o === null ? -1 : o.v;
}
class Util { id<T>(x: T): T { return x; } }
class UtilMain { id<T>(x: T): T { return x; } }
function viaMethod<U extends R>(x: U): i32 {
  const o = new Util().id(x).m(); return o === null ? -1 : o.v;
}
function viaMethodMain<U extends R>(x: U): i32 {
  const o = new UtilMain().id(x).m(); return o === null ? -1 : o.v;
}
function choose<U extends R, V extends R>(x: U, y: V, c: boolean): i32 {
  const z = c ? x : y;
  return z === x ? 1 : 0;
}
function choosePlain<U extends R>(x: U, c: boolean): i32 {
  const z = c ? x : new K();
  return z === x ? 1 : 0;
}
export function main(): void {
  const k = new K();
  const k0 = new K0();
  print(`class:${g(k)}:${g(k0)}`);
  print(`function:${h(k)}:${h(k0)}`);
  print(`array:${arrays(k)}:${arrays(k0)}`);
  print(`map:${maps(k)}:${maps(k0)}`);
  print(`field:${new B(k).run()}:${new B(k0).run()}`);
  print(`loop:${loop([k])}:${loop([k0])}`);
  print(`element:${element([k])}:${element([k0])}`);
  print(`narrowed:${narrowed(k)}:${narrowed(k0)}`);
  print(`returned:${returned(k)}:${returned(k0)}`);
  print(`first-constrained:${viaFirst([k])}:${first([k]).m().v}`);
  print(`first-main:${firstMain([k]).m().v}:${viaFirstMain([k])}`);
  print(`pick-constrained:${viaPick(k)}:${pick(k).m().v}`);
  print(`pick-main:${pickMain(k).m().v}:${viaPickMain(k)}`);
  print(`method-constrained:${viaMethod(k)}:${new Util().id(k).m().v}`);
  print(`method-main:${new UtilMain().id(k).m().v}:${viaMethodMain(k)}`);
  print(`conditional:${choose(k, k, true)}:${choose(k, k, false)}`);
  print(`conditional-plain:${choosePlain(k, true)}:${choosePlain(k, false)}`);
  print(`both:${both(k, k)}:${both(k0, k0)}:${both(k, k0)}:${both(k0, k)}`);
}
