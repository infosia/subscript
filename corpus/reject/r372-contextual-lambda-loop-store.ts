// corpus: reject/r372-contextual-lambda-loop-store
// purpose: Rejects a loop store from a contextually typed lambda parameter.
// exercises: nullable-reference, narrowing-flow
// questions: compiler.md §162 rule 3a, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 16, may be null here

class A { x: i32 = 1; }
export function main(): void {
  let b: A | null = new A();
  for (let i = 0; i < 2; i++) {
    const xs = [b];
    xs.forEach((v): void => {
      let a: A | null = new A();
      for (let j = 0; j < 2; j++) {
        print(`${a.x}`);
        a = v;
      }
    });
    b = null;
  }
}
