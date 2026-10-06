// corpus: reject/r380-dropped-indirect-async-handle
// purpose: A call through a function value must have an awaited completion.
// exercises: async-function-value, dropped-handle
// questions: compiler.md §167 rule 7, §70
// tsc: accepts
// expected-error: S013, the call through job drops an async handle

async function value(n: i32): Promise<i32> { return n; }
export function main(): void {
  const job: (n: i32) => Promise<i32> = value;
  job(7);
}
