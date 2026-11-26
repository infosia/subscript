// corpus: warn/w06-array-use-after-free
// warning: W002
// purpose: Identifies a straight-line read of a local array after its explicit release.
// exercises: Context.free, dynamic-array, use-after-free, local-dataflow
// questions: Q6, compiler.md §358

export function main(): void {
  const values: i32[] = [1, 2, 3];
  Context.free(values);
  print(`${values[0]}`);
}
