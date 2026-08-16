// corpus: accept/a139-using-async
// purpose: Keeps a disposable binding live across one suspension.
// exercises: using-declaration, symbol-dispose, async-function, direct-await
// questions: §60, R31

class AsyncResource {
  [Symbol.dispose](): void {
    print("dispose:async");
  }
}

export async function main(): Promise<void> {
  using resource = new AsyncResource();
  print("before");
  await Context.suspend();
  print("resumed");
}
