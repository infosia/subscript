// corpus: reject/r14-async
// purpose: Rejects async functions that require an event loop.
// exercises: rejected-async, promise
// questions: none
// expected-error: no event loop; use coroutines

async function compute(): Promise<i32> {
  return 7;
}

export function main(): void {
  const pending: Promise<i32> = compute();
  print(pending instanceof Promise ? "async" : "unexpected");
}
