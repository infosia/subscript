// corpus: trap/t122-element-read-after-free-q6
// purpose: Dev-JIT traps when an element is read through a released array.
// exercises: Context.free, dynamic-array, element-read, use-after-delete, Q6-tier-carve-out
// questions: Q6, compiler.md §358
// tier-policy: dev-JIT traps with freed-handle diagnostics enabled; ship-C-AOT behavior is deliberately unspecified
// expected-trap: use-after-delete at the released array element read

export function main(): void {
  const values: i32[] = [11, 22, 33];
  Context.free(values);
  print("before released read");
  print(`${values[2]}`);
  print("after released read");
}
