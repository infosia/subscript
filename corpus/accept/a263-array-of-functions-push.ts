// corpus: accept/a263-array-of-functions-push
// interpreter: no — the reference interpreter cannot pack function values into arrays
// purpose: Store a named function without a callback raise edge (compiler.md §118.2).
// exercises: array, function-parameter, indirect-call
// questions: Q10
// tsc: accepts; js-comparable: yes
function named(): i32 { return 7; }
function store(values: (() => i32)[]): void { values.push(named); }
export function main(): void {
  const values: (() => i32)[] = [];
  values.push(named);
  store(values);
  print(`${values.length} ${values[0]()} ${values[1]()}`);
}
