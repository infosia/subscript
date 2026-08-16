// corpus: reject/r132-await-using
// purpose: Rejects an await using declaration.
// exercises: await-using-declaration, symbol-dispose, async-function
// questions: §60, R31
// expected-error: await using is outside the decided surface

class AsyncDisposableResource {
  [Symbol.dispose](): void {}
}

export async function main(): Promise<void> {
  await using resource = new AsyncDisposableResource();
}
