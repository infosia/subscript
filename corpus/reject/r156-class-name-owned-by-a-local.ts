// corpus: reject/r156-class-name-owned-by-a-local
// purpose: Rejects construction through a local that owns a known class name.
// exercises: class-name, local-shadow, constructor-read
// questions: §67
// tsc: rejects TS2351
// expected-error: S100 at the constructor read

class Foo {
  value: i32 = 1;
}

export function main(): void {
  const Foo: i32 = 9;
  const item: Foo = new Foo();
  print(`${item.value}:${Foo}`);
}
