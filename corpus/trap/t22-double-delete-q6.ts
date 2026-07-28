// corpus: trap/t22-double-delete-q6
// purpose: Dev-JIT traps when Context.free releases an already-released allocation.
// exercises: Context.free, double-delete, Q6-tier-carve-out
// questions: Q6
// tier-policy: dev-JIT traps; ship-C-AOT behavior is deliberately unspecified
// expected-trap: double-delete at the second Context.free call

class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const value: Box = new Box(7);
  Context.free(value);
  print("before second delete");
  Context.free(value);
  print("after second delete");
}
