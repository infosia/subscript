// corpus: accept/a428-value-lambda-and-conditional-take-a-void-result-place
// purpose: A lambda with a value result annotation, and a conditional of function values, take a place whose function type has the result `void`; the call discards the result.
// observable: each callback prints its argument, so the order of calls is visible; a `return` operand with an effect shows that it runs once.
// exercises: function-value, lambda, void, forEach, Map, conditional, static-method
// questions: compiler.md §242, §240, §237
// tsc: accepts; js-comparable: yes
type CB = (x: i32) => void;

function named(x: i32): i32 {
  print(`named${x}`);
  return x * 2;
}

function other(x: i32): void {
  print(`other${x}`);
}

class K {
  static twice(x: i32): i32 {
    print(`twice${x}`);
    return x * 2;
  }
}

function each(xs: i32[], f: (x: i32) => void): void {
  for (const x of xs) {
    f(x);
  }
}

export function main(): void {
  const flag: boolean = true;
  const m = new Map<string, i32>();
  m.set("a", 5);
  [1].forEach(flag ? named : K.twice);
  [2].forEach(flag ? other : named);
  [3].forEach(!flag ? (x: i32): i32 => x : named);
  m.forEach(
    flag
      ? (v: i32, k: string): void => {
          print(`entry${k}=${v}`);
        }
      : (v: i32, k: string): i32 => v,
  );
  each([6], (x: i32): i32 => {
    print(`lambda${x}`);
    return named(x);
  });
  [7].forEach((x: i32): i32 => {
    print(`${x}`);
    return x;
  });
  const cb: CB = (x: i32): i32 => named(x);
  cb(8);
  const r: CB = flag ? (x: i32): i32 => K.twice(x) : other;
  r(9);
}
