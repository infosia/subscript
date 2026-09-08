// corpus: reject/r187-async-generic-method-on-value-class
// purpose: Rejects an async method with type parameters on a @CStruct value class.
// exercises: generic-method, async-method, value-class
// questions: §93, §37.1
// tsc: accepts
// expected-error: S100 at the method declaration
@CStruct
class Vec2 {
  x: f32 = 0;
  y: f32 = 0;

  async load<T>(value: T): Promise<T> {
    await Context.suspend();
    return value;
  }
}

export async function main(): Promise<void> {
  const v: Vec2 = new Vec2();
  print(`${await v.load<i32>(1)}`);
}
