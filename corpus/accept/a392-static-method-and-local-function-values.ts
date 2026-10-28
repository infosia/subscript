// corpus: accept/a392-static-method-and-local-function-values
// purpose: A static method is a function value, and a local function that reads no local of its enclosing function is a module function.
// exercises: function-values, static-methods, local-functions, hoisting, recursion
// questions: compiler.md §207
// tsc: accepts; js-comparable: yes
class M {
  static twice(x: i32): i32 {
    return x * 2;
  }
  static base: i32 = 100;
  static plusBase(x: i32): i32 {
    return x + M.base;
  }
}

class V {
  x: i32 = 0;
  static make(x: i32): V {
    const v = new V();
    v.x = x;
    return v;
  }
}

let calls: i32 = 0;

function apply(f: (x: i32) => i32, v: i32): i32 {
  calls += 1;
  return f(v);
}

function outer(n: i32): i32 {
  const early = sq(n);
  function sq(x: i32): i32 {
    return x * x;
  }
  function fact(x: i32): i32 {
    return x <= 1 ? 1 : x * fact(x - 1);
  }
  function isEven(x: i32): boolean {
    return x === 0 ? true : isOdd(x - 1);
  }
  function isOdd(x: i32): boolean {
    return x === 0 ? false : isEven(x - 1);
  }
  return early + fact(n) + (isEven(n) ? 1000 : 0) + apply(sq, n + 1);
}

function picker(flag: boolean): (x: i32) => i32 {
  function inc(x: i32): i32 {
    return x + 1;
  }
  function dec(x: i32): i32 {
    return x - 1;
  }
  return flag ? inc : dec;
}

function* evens(limit: i32): Generator<i32> {
  function half(x: i32): i32 {
    return x / 2;
  }
  for (let i: i32 = 0; i < limit; i += 2) {
    yield half(i);
  }
}

export function main(): void {
  print(`${apply(M.twice, 4)} ${apply(M.plusBase, 1)} ${calls}`);
  const g = M.twice;
  const fs: ((x: i32) => i32)[] = [M.twice, M.plusBase];
  M.base = 7;
  print(`${g(5)} ${fs[1](1)}`);
  const mk = V.make;
  print(`${mk(3).x}`);
  print(`${outer(4)} ${outer(3)}`);
  print(`${picker(true)(10)} ${picker(false)(10)}`);
  const seen: i32[] = [];
  for (const v of evens(7)) {
    seen.push(v);
  }
  print(seen.join(","));
}
