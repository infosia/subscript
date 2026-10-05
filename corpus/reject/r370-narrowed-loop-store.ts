// corpus: reject/r370-narrowed-loop-store
// purpose: Rejects a loop store whose value needs a provisional narrowing fact.
// exercises: nullable-reference, narrowing-flow
// questions: compiler.md §162 rule 3a, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 12, may be null here

class A { x: i32 = 1; }
export function main(): void {
  let a: A | null = new A();
  let b: A | null = new A();
  for (let i: i32 = 0; i < 3; i++) { print(`${a.x}`); a = b; b = null; }
}
