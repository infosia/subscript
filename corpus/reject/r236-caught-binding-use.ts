// corpus: reject/r236-caught-binding-use
// purpose: Rejects a read of the catch binding outside `instanceof` and `throw`, because the binding has no type before a narrowing test.
// exercises: try-catch, catch-binding, exception
// questions: Q9
// tsc: accepts
// expected-error: S010 at the use of `e`
export function main(): void {
  try {
    throw new Error("failure");
  } catch (e) {
    const copy = e;
    print("handler");
  }
}
