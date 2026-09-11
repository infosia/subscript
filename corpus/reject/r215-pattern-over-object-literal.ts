// corpus: reject/r215-pattern-over-object-literal
// purpose: Rejects a field pattern over an anonymous object source.
// exercises: binding-pattern, object-literal
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the undecided object-literal surface
export function main(): void {
  const { x } = { x: 1 };
  print(`${x}`);
}
