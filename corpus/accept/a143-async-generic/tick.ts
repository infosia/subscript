// corpus: accept/a143-async-generic/tick
// purpose: Exports a generic async function for module calls.
// exercises: generic-async-function
// questions: R36, compiler.md §129
// tsc: accepts; js-comparable: no C2 C8: The entry uses ValueType.
export async function tick<T>(): Promise<void> {
  print("tick");
}
