// corpus: trap/t23-use-after-delete-q6
// purpose: Dev-JIT traps when a field is read through a released reference.
// exercises: unsafeDelete, reference-field-read, use-after-delete, Q6-tier-carve-out
// questions: Q6
// tier-policy: dev-JIT traps; ship-C-AOT behavior is deliberately unspecified
// expected-trap: use-after-delete at the released reference read

class Box {
  first: i32;
  second: i32;
  third: i32;
  constructor(first: i32, second: i32, third: i32) {
    this.first = first;
    this.second = second;
    this.third = third;
  }
}

export function main(): void {
  const value: Box = new Box(11, 22, 33);
  unsafeDelete(value);
  print("before released read");
  print(`${value.third}`);
  print("after released read");
}
