// corpus: reject/r286-generic-body-member-on-type-parameter
// purpose: Rejects a member read on a type parameter although every instance has the member.
// exercises: generic-function, type-parameter, member-read
// questions: compiler.md §135
// tsc: rejects TS2339
// expected-error: S018 at line 12, the member v of the type parameter T
class Box {
  v: i32 = 5;
}

function read<T>(value: T): i32 {
  return value.v;
}

export function main(): void {
  print(`${read<Box>(new Box())}`);
}
