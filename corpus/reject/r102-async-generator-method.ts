// corpus: reject/r102-async-generator-method
// purpose: Keeps async generators outside the async instance-method surface.
// exercises: async-generator-method
// questions: R13, Q34, C8
// expected-error: S100 at the async generator method

class Worker {
  async *values(): AsyncGenerator<i32> {
    yield 1;
  }
}

export function main(): void {}
