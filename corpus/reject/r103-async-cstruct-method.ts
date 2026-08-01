// corpus: reject/r103-async-cstruct-method
// purpose: Keeps async frames off value-class receivers.
// exercises: async-method, CStruct-value-class
// questions: R13, Q34, C2
// expected-error: S100 at the async value-class method

@CStruct
class ValueWorker {
  async work(): Promise<void> {
    await Context.suspend();
  }
}

export function main(): void {}
