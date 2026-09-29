// corpus: reject/r103-async-valuetype-method
// purpose: Keeps async frames off value-class receivers.
// exercises: async-method, ValueType-value-class
// questions: R13, Q34, C2
// tsc: accepts
// expected-error: S100 at the async value-class method
@ValueType
class ValueWorker {
  async work(): Promise<void> {
    await Context.suspend();
  }
}

export function main(): void {}
