// corpus: reject/r186-async-generic-method-without-type-args
// purpose: Rejects an await of an async generic method whose type parameter has no candidate.
// exercises: generic-method, async-method
// questions: §93, §82.4, compiler.md §197
// tsc: accepts
// expected-error: S100, cannot infer type parameter `T` of `Loader.load`: no candidate
class Loader {
  async load<T>(): Promise<T[]> {
    await Context.suspend();
    return [];
  }
}

export async function main(): Promise<void> {
  const loader: Loader = new Loader();
  print(`${(await loader.load()).length}`);
}
