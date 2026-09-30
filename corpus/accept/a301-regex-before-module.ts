// corpus: accept/a301-regex-before-module
// purpose: Regex literal globals initialize before all module work.
// observable: The global initializer prints true through the later function.
// exercises: regex-literal-global, initializer-call-route
// questions: compiler.md §137 rule 3b, C18
// tsc: accepts; js-comparable: yes
const matched: boolean = hit();
function hit(): boolean {
  return /a+/.test("aaa");
}
export function main(): void {
  print(`${matched}`);
}
