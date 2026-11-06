// corpus: reject/r484-or-fact-on-one-arm
// purpose: A fact that only one arm of || gives does not narrow the path.
// exercises: logical-or-narrowing, nullable-reference
// questions: compiler.md §227.1 rule 6, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 10, may be null here

function lengthOf(a: string | null, b: string | null): i32 {
  if (a === "x" || b === "y") {
    return a.length;
  }
  return -1;
}

export function main(): void {
  print(`${lengthOf("x", null)}`);
}
