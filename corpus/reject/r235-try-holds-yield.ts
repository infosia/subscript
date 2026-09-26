// corpus: reject/r235-try-holds-yield
// purpose: Rejects a `yield` inside a `try` block.
// exercises: try-catch, yield, generator, suspension, exception
// questions: Q9, Q11
// tsc: accepts
// expected-error: S010 at the `yield`
function* numbers(): Generator<i32> {
  try {
    yield 1;
  } catch (e) {
    print("handler");
  }
}

export function main(): void {
  const values: Generator<i32> = numbers();
  values.next();
}
