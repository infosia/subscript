// corpus: reject/r417-constructor-handle-parameter
// purpose: A constructor parameter that holds handles is an origin; a constructor that reads only its length drops them.
// exercises: async-handle, dropped-handle, class, constructor
// questions: compiler.md §188 rule 2, compiler.md §70, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function work(v: i32): Promise<i32> { return v; }
class Counter {
  n: i32;
  constructor(jobs: Promise<i32>[]) { this.n = jobs.length; }
}
export async function main(): Promise<void> {
  const c = new Counter([work(1)]);
  print(`${c.n} end`);
}

// pin: 5149dfb3
// pin-dev-jit: Accepted; stdout `1 end`.
// pin-c-aot: Accepted; stdout `1 end`.
// pin-interpreter: Accepted; stdout `1 end`.
