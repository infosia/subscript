// corpus: accept/a462-loop-store-order
// purpose: A whole local keeps its fact at each loop back edge.
// exercises: loop-head, assignment, null-check
// questions: compiler.md §315, collisions.md C7
// tsc: accepts; js-comparable: yes

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
export function main(): void {
  let e: A | null = mk();
  if (e === null) { return; }
  for (let i = 0; i < 3; i++) {
    print(`${e.x}`); e = mk(); e = new A();
  }
}
