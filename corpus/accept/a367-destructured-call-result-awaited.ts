// corpus: accept/a367-destructured-call-result-awaited
// purpose: A pattern binding carries the origin of a call result; an await through a binding discharges it.
// exercises: async-handle, binding-pattern
// questions: compiler.md §188 rule 2, compiler.md §107, collisions.md C8
// tsc: accepts; js-comparable: yes
async function work(v: i32): Promise<i32> { return v; }
function many(): Promise<i32>[] { return [work(1), work(2)]; }
export async function main(): Promise<void> {
  const [a, b] = many();
  print(`${await a} ${await b}`);
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Exit 1; 1 error, the only one S013 "an async handle is dropped without any await of its completion" at 9:18.
// pin-c-aot: Exit 1; the same 1 error before C emission.
// pin-interpreter: Checker rejects with the same 1 error before lowering.
