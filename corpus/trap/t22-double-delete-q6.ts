// corpus: trap/t22-double-delete-q6
// purpose: Dev-JIT traps when unsafeDelete releases an already-released allocation.
// exercises: unsafeDelete, double-delete, Q6-tier-carve-out
// questions: Q6
// tier-policy: dev-JIT traps; ship-C-AOT behavior is deliberately unspecified
// expected-trap: double-delete at the second unsafeDelete call

class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const value: Box = new Box(7);
  unsafeDelete(value);
  print("before second delete");
  unsafeDelete(value);
  print("after second delete");
}
