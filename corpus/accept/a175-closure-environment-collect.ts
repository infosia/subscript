// corpus: accept/a175-closure-environment-collect
// purpose: A capturing lambda's environment stays valid across Context.collect.
// observable: Calls through two capturing lambdas and a callback print the captured string after collection.
// exercises: capturing-lambda, explicit-collection, closure-environment, function-value-intrinsic
// questions: Q7, C5
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
export function main(): void {
  const name: string = `n${100 + 23}`;
  const f = (x: i32): string => `${name}-${x}`;
  Context.collect();
  const g = (x: i32): string => f(x + 1);
  Context.collect();
  print(`${g(1)} ${f(2)}`);
  const xs: i32[] = [1, 2, 3];
  const ys: string[] = xs.map((x: i32): string => { Context.collect(); return f(x); });
  print(`${ys[0]} ${ys[2]}`);
}
