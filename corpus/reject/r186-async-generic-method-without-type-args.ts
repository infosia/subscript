// corpus: reject/r186-async-generic-method-without-type-args
// purpose: Rejects an await of an async generic method that supplies no type arguments.
// exercises: generic-method, async-method
// questions: §93, §82.4
// tsc: accepts
// expected-error: S100 at the call
class Loader {
  async load<T>(value: T): Promise<T> {
    await Context.suspend();
    return value;
  }
}

export async function main(): Promise<void> {
  const loader: Loader = new Loader();
  print(`${await loader.load(1)}`);
}
