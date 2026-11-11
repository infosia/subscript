// corpus: accept/a434-a-generic-argument-takes-the-type-that-other-arguments-fix
// purpose: An argument of a generic call takes its parameter type with the type parameter that the other arguments fix, in either order.
// observable: each lambda is called and prints, so the parameter type that it takes is visible.
// exercises: generic-function, type-inference, lambda, array-literal, function-value
// questions: compiler.md §249 rule 2a, §212, §197
// tsc: accepts; js-comparable: yes
type CB = (x: i32) => void;

function one(x: i32): void {
  print(`one ${x}`);
}

function two<T>(a: T, xs: T[]): T[] {
  return xs;
}

function twoR<T>(xs: T[], a: T): T[] {
  return xs;
}

function firstOr<T>(xs: T[], fallback: T): T {
  return fallback;
}

function pair<T>(a: T, b: T): T {
  return b;
}

function lists(): void {
  const r = two(one, [one, (x) => print(`two ${x}`)]);
  r[0](1);
  r[1](2);
  const s = twoR([one, (x) => print(`twoR ${x}`)], one);
  s[0](3);
  s[1](4);
  const cs: CB[] = r;
  cs[1](5);
}

function values(): void {
  const handlers = [(x: i32): i32 => x * 10];
  const f = firstOr(handlers, (x) => x + 1);
  print(`firstOr ${f(6)}`);
  const g = pair((x: i32): i32 => x * 2, (x) => x + 100);
  print(`pair ${g(7)}`);
  const h = pair((x) => x + 1000, (x: i32): i32 => x * 3);
  print(`pair ${h(8)}`);
}

export function main(): void {
  lists();
  values();
}
