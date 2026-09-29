// corpus: reject/r269-import-assignment/lib
// purpose: Keeps the exporter's global mutable.
// exercises: module-export
// questions: compiler.md §127

export let count: i32 = 4;
export function bump(): void { count += 1; }
