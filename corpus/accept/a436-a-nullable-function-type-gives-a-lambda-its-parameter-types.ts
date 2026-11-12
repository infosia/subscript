// corpus: accept/a436-a-nullable-function-type-gives-a-lambda-its-parameter-types
// purpose: A contextual type F | null, where F is a function type, gives a lambda the parameter types and the result context of F.
// observable: each lambda is called and prints, so the parameter type that it takes is visible.
// exercises: lambda, nullable, function-value, generic-function, conditional, array-literal, Map
// questions: compiler.md §251, §249 rule 2a, §212 rule 1a
// tsc: accepts; js-comparable: yes
type CB = (x: i32) => void;

function one(x: i32): void {
  print(`one ${x}`);
}

function maybe(): CB | null {
  return one;
}

function three<T>(a: T, b: T, c: T): T {
  return c;
}

function retn(): CB | null {
  return (x) => print(`r ${x}`);
}

function take(f: CB | null): void {
  if (f !== null) {
    f(4);
  }
}

class Holder {
  cb: CB | null = null;
}

function call(f: CB | null, x: i32): void {
  if (f !== null) {
    f(x);
  }
}

function pick(n: i32): boolean {
  return n > 0;
}

export function main(): void {
  const f: CB | null = (x) => print(`f ${x}`);
  call(f, 1);
  let g: CB | null = null;
  g = (x) => print(`g ${x}`);
  call(g, 2);
  call(retn(), 3);
  take((x) => print(`t ${x}`));
  const h3 = three(one, maybe(), (x) => print(`h ${x}`));
  call(h3, 5);
  const k3 = three(maybe(), one, (x) => print(`k ${x}`));
  call(k3, 6);
  const b = pick(1);
  const c: CB | null = b ? (x) => print(`c ${x}`) : null;
  call(c, 7);
  const holder = new Holder();
  holder.cb = (x) => print(`hc ${x}`);
  call(holder.cb, 8);
  const xs: (CB | null)[] = [(x) => print(`xs ${x}`), null];
  call(xs[0], 9);
  call(xs[1], 10);
  const m = new Map<string, CB | null>();
  m.set("k", (x) => print(`m ${x}`));
  for (const v of m.values()) {
    call(v, 11);
  }
  const d: ((x: i32) => i32) | null = (x) => x * 2;
  if (d !== null) {
    print(`d ${d(21)}`);
  }
}
