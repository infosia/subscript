// corpus: accept/a466-optional-function-context
// purpose: An optional function result gives a lambda its parameter and result context.
// observable: The returned lambda adds one to its argument.
// exercises: lambda, nullable, undefined, function-value
// questions: compiler.md §318 rule 1
// tsc: accepts; js-comparable: yes
type CB = (x: i32) => i32;
function r(): CB | undefined { return (x) => x + 1; }

export function main(): void {
  const f = r();
  if (f !== undefined) print(`${f(41)}`);
}
