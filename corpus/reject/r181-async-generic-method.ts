// corpus: reject/r181-async-generic-method
// purpose: Rejects an async method that declares type parameters.
// exercises: generic-method, async-method
// questions: §82.4, §64
// tsc: accepts
// expected-error: S100 at the method declaration
class Box {
  async load<T>(value: T): Promise<T> {
    await Context.suspend();
    return value;
  }
}

export async function main(): Promise<void> {
  const box: Box = new Box();
  print(`${await box.load<i32>(1)}`);
}
