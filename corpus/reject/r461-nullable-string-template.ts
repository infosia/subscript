// corpus: reject/r461-nullable-string-template
// purpose: A template operand of type `string | null` is rejected with a text that states the fix; `node` prints the string `null` there.
// exercises: nullable-string, template-interpolation
// questions: compiler.md §218
// tsc: accepts
// expected-error: S100, type `string | null` cannot be interpolated into a template; a null test first gives a `string`
function label(value: string | null): string {
  return `<${value}>`;
}

export function main(): void {
  print(label(null));
}
