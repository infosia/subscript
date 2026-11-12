// corpus: accept/a438-each-fixedarray-type-has-a-c-form
// purpose: A FixedArray type that only an empty dynamic array or a Map names has a C form on the ship tier.
// observable: each container prints its size, so the program runs to its end on each tier.
// exercises: FixedArray, dynamic-array, Map
// questions: compiler.md §254.1 rule 2
// tsc: accepts; js-comparable: yes
export function main(): void {
  const e: FixedArray<i32, 2>[] = [];
  const m = new Map<string, FixedArray<i32, 3>>();
  print(`sizes ${e.length} ${m.size}`);
}
