// corpus: reject/r234-try-holds-await
// purpose: Rejects an `await` inside a `try` block.
// exercises: try-catch, await, suspension, exception
// questions: Q9, Q34
// tsc: accepts
// expected-error: S010 at the `await`
export async function main(): Promise<void> {
  try {
    await Context.suspend();
  } catch (e) {
    print("handler");
  }
}
