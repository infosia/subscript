// corpus: accept/a187-async-generic-method
// purpose: Proves an async method with type parameters at two instantiations, awaited directly and through a held handle.
// exercises: generic-method, async-method, monomorphization, held-async-handle
// questions: §93, §82.4, Q34
// tsc: accepts; js-comparable: no C2 C8: The CStruct decorator has no JavaScript shim.
@CStruct
class Vec2 {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
}

class Loader {
  calls: i32 = 0;

  async load<T>(value: T, label: string): Promise<T> {
    print(`load:${label}:start`);
    await Context.suspend();
    this.calls = this.calls + 1;
    print(`load:${label}:resume`);
    return value;
  }
}

export async function main(): Promise<void> {
  const loader: Loader = new Loader();

  const number: i32 = await loader.load<i32>(7, "i32");
  print(`number=${number}`);

  const held: Promise<Vec2> = loader.load<Vec2>(new Vec2(1.5, 2.5), "vec2");
  print("main:held");
  const vector: Vec2 = await held;
  print(`vector=${vector.x},${vector.y}`);

  print(`calls=${loader.calls}`);
}
