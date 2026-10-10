// corpus: accept/a365-handle-global-store-awaited
// purpose: A store of a handle into a module global discharges its origin; a later await of the global observes the task.
// exercises: async-handle, module-global
// questions: compiler.md §188 rule 3, compiler.md §70, collisions.md C8
// tsc: accepts; js-comparable: yes
async function work(v: i32): Promise<i32> { return v; }
let G: Promise<i32>[] = [];
function start(): void { G = [work(1), work(2)]; }
export async function main(): Promise<void> {
  start();
  print(`${await G[0]} ${await G[1]}`);
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Exit 1; 2 errors, the first S013 "an async handle is dropped without any await of its completion" at 8:31.
// pin-c-aot: Exit 1; the same 2 errors before C emission.
// pin-interpreter: Checker rejects with the same 2 errors before lowering.
