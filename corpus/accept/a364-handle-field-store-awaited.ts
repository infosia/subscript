// corpus: accept/a364-handle-field-store-awaited
// purpose: A store of a handle into a field discharges its origin; a later await of the field observes the task.
// exercises: async-handle, class, field
// questions: compiler.md §188 rule 3, compiler.md §70, collisions.md C8
// tsc: accepts; js-comparable: yes
async function work(v: i32): Promise<i32> { return v; }
class Holder {
  job: Promise<i32>;
  constructor(job: Promise<i32>) { this.job = job; }
}
export async function main(): Promise<void> {
  const h = new Holder(work(0));
  print(`${await h.job}`);
  h.job = work(1);
  print(`${await h.job}`);
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Exit 1; 1 error, the only one S013 "an async handle is dropped without any await of its completion" at 14:11.
// pin-c-aot: Exit 1; the same 1 error before C emission.
// pin-interpreter: Checker rejects with the same 1 error before lowering.
