// corpus: accept/a325-generic-inference-null
// purpose: Joins null with a generic inference candidate in either argument order.
// exercises: generic-function, generic-inference, nullable
// questions: compiler.md §149
// tsc: accepts; js-comparable: yes
class Box { value: i32 = 7; }
function pair<T>(a: T, b: T): T { return a; }
export function main(): void {
  const a: Box | null = pair(null, new Box());
  const explicitA: Box | null = pair<Box | null>(null, new Box());
  print(`${a == null} ${explicitA == null}`);
  const b: Box | null = pair(new Box(), null);
  const explicitB: Box | null = pair<Box | null>(new Box(), null);
  if (b != null && explicitB != null) { print(`${b.value} ${explicitB.value}`); }
}
