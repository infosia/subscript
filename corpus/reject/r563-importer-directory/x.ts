// corpus: reject/r563-importer-directory/x
// purpose: The only x module; it is beside main.ts, not in sub/.
// exercises: module-export
// questions: compiler.md §354 rule 2
export const x: i32 = 1;
