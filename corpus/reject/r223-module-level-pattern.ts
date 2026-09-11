// corpus: reject/r223-module-level-pattern
// purpose: Rejects a binding pattern in a module-level declaration.
// exercises: binding-pattern, module-global
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the function body a pattern binds inside
const values: i32[] = [1, 2];
const [first, second] = values;
export function main(): void {
  print(`${first} ${second}`);
}
