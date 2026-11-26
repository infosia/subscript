// corpus: trap/t121-double-free-array-q6
// purpose: Dev-JIT traps when Context.free releases an already-released array.
// exercises: Context.free, dynamic-array, double-delete, Q6-tier-carve-out
// questions: Q6, compiler.md §358
// tier-policy: dev-JIT traps with freed-handle diagnostics enabled; ship-C-AOT behavior is deliberately unspecified
// expected-trap: double-delete at the second Context.free call

export function main(): void {
  const values: i32[] = [1, 2, 3];
  Context.free(values);
  print("before second free");
  Context.free(values);
  print("after second free");
}
