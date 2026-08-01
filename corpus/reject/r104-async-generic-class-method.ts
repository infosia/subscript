// corpus: reject/r104-async-generic-class-method
// purpose: Keeps async methods off generic class templates.
// exercises: async-method, generic-class-template
// questions: R13, Q34
// expected-error: S100 at the async generic-class method

class Worker<T> {
  async work(value: T): Promise<T> {
    await Context.suspend();
    return value;
  }
}

export function main(): void {}
