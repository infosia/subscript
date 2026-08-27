// corpus: reject/r101-async-static-method
// purpose: Keeps async methods confined to instance dispatch.
// exercises: async-static-method
// questions: R13, Q34
// tsc: accepts
// expected-error: S100 at the async static method
class Worker {
  static async work(): Promise<void> {
    await Context.suspend();
  }
}

export function main(): void {}
