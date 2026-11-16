// corpus: accept/a470-a-function-with-defaults-fits-a-place-with-fewer-parameters
// purpose: A function with trailing defaults takes an adapter in a place with fewer parameters.
// observable: each adapted call prints its result; a default prints once per omitted argument.
// exercises: default-parameter, function-value, capturing-lambda, built-in-callback
// questions: compiler.md §320
// tsc: accepts; js-comparable: yes
function f(a: i32, b: i32 = 10): i32 { return a + b; }
function app(k: (a: i32) => i32): i32 { return k(1); }
const g = (a: i32, b: i32 = 20): i32 => a * b;

function tick(): i32 { print("default"); return 30; }
function effect(a: i32, b: i32 = tick()): i32 { return a + b; }
function grp(a: i32, i: i32 = 99): string { return `${a}:${i}`; }

export function main(): void {
  print(`${app(f)}`);
  print(`${app(g)}`);
  const h: (a: i32) => i32 = f;
  print(`${h(2)}`);
  print(`${app(effect)}`);
  print(`${app(effect)}`);
  const offset: i32 = 40;
  const local = (a: i32, b: i32 = tick()): i32 => a + b + offset;
  print(`${app(local)}`);
  print(`${app(local)}`);
  const indexed = [1, 2].map(f);
  print(`${indexed[0]},${indexed[1]}`);
  const groups = Map.groupBy([5, 6], grp);
  for (const key of groups.keys()) { print(key); }
}
