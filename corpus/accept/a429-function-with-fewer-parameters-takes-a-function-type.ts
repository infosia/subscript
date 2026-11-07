// corpus: accept/a429-function-with-fewer-parameters-takes-a-function-type
// purpose: A lambda literal, a function declaration, and a static method with fewer parameters take a place whose function type has more parameters.
// observable: each callback prints its arguments, so the order of calls and the argument values are visible.
// exercises: function-value, lambda, static-method, void, arity
// questions: compiler.md §243, §240, §235
// tsc: accepts; js-comparable: yes
type CB2 = (x: i32, i: i32) => void;
type F2 = (x: i32, i: i32) => i32;

function eachi(xs: i32[], f: (x: i32, i: i32) => void): void {
  let i: i32 = 0;
  for (const x of xs) {
    f(x, i);
    i++;
  }
}

function one(x: i32): void {
  print(`one${x}`);
}

function twice(x: i32): i32 {
  print(`twice${x}`);
  return x * 2;
}

class K {
  static one(x: i32): void {
    print(`K.one${x}`);
  }
}

export function main(): void {
  const n: i32 = 10;
  eachi([1], (x: i32): void => print(`l${x}`));
  eachi([2], (x) => print(`u${x}`));
  eachi([3], () => print("z"));
  eachi([4], one);
  eachi([5], K.one);
  eachi([1], twice);
  const f: CB2 = one;
  f(6, 0);
  const t: F2 = twice;
  print(`t=${t(7, 0)}`);
  const r: F2 = (x: i32): i32 => x + 1;
  print(`r=${r(8, 0)}`);
  const g: CB2 = (x: i32): void => print(`${x + n}`);
  g(9, 0);
}
