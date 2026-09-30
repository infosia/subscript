// corpus: reject/r284-generic-body-const-assignment
// purpose: Rejects a const assignment in a generic function body that no call instantiates.
// exercises: generic-function, const-binding
// questions: compiler.md §135
// tsc: rejects TS2588
// expected-error: S100 at line 10, the assignment to the const binding limit
const limit: i32 = 1;

function reset<T>(value: T): i32 {
  limit = 2;
  return 0;
}

export function main(): void {
  print(`${limit}`);
}
