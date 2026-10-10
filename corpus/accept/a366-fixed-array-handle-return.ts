// corpus: accept/a366-fixed-array-handle-return
// purpose: A return of a FixedArray of handles discharges the origins in the callee; the caller awaits each element.
// exercises: async-handle, fixed-array, return
// questions: compiler.md §188 rule 1, compiler.md §188 rule 3, collisions.md C8
// tsc: accepts; js-comparable: yes
async function work(v: i32): Promise<i32> { return v; }
function pair(): FixedArray<Promise<i32>, 2> {
  const jobs: FixedArray<Promise<i32>, 2> = [work(1), work(2)];
  return jobs;
}
export async function main(): Promise<void> {
  const p = pair();
  print(`${await p[0]} ${await p[1]}`);
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Exit 1; 2 errors, the first S013 "an async handle is dropped without any await of its completion" at 8:46.
// pin-c-aot: Exit 1; the same 2 errors before C emission.
// pin-interpreter: Checker rejects with the same 2 errors before lowering.
