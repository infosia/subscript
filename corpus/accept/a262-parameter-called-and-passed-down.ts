// corpus: accept/a262-parameter-called-and-passed-down
// purpose: Call a callback and pass it downward through two levels (compiler.md §118).
// exercises: lambda, capture, function-parameter
// questions: Q10
// tsc: accepts; js-comparable: yes
function leaf(cb: () => i32): i32 { return cb(); }
function middle(cb: () => i32): i32 { return cb() + leaf(cb); }
function outer(cb: () => i32): i32 { return cb() + middle(cb); }
function named(): i32 { return 7; }
export function main(): void {
  const value: i32 = 5;
  print(`${outer((): i32 => value)}`);
  print(`${outer(named)}`);
}
