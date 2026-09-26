// corpus: reject/r233-finally
// purpose: Rejects a `finally` clause, which the decided exception surface does not hold.
// exercises: try-catch, finally, exception
// questions: Q9
// tsc: accepts
// expected-error: S010 at the `finally` block
export function main(): void {
  try {
    print("body");
  } catch (e) {
    print("handler");
  } finally {
    print("cleanup");
  }
}
