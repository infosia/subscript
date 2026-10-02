// corpus: accept/a313-generator-done-wire-zero
// purpose: Wire aliases with a zero member and plain aliases read their zero after completion.
// exercises: generator, iterator-result, CEnum, fixed-array, member-read, string-alias
// questions: compiler.md §145, collisions.md C8
// tsc: accepts; js-comparable: no C8: A finished alias generator returns zero instead of undefined.
type W = CEnum<{ w3: 3; w0: 0; }>;
type P = "first" | "second";
function* wires(): Generator<W> { yield "w3"; }
function* fixed(): Generator<FixedArray<W, 2>> { yield ["w3", "w3"]; }
function* plain(): Generator<P> { yield "second"; }
export function main(): void {
  const wire = wires();
  print(`${wire.next().value}`);
  print(`${wire.next().value}`);
  const array = fixed();
  array.next();
  const r = array.next();
  const value = r.value;
  print(`${value[0]}|${value[1]}`);
  const alias = plain();
  alias.next();
  print(`${alias.next().value}`);
}
