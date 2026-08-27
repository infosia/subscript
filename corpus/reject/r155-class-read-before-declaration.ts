// corpus: reject/r155-class-read-before-declaration
// purpose: Rejects a class-name read before a local declaration owns the name.
// exercises: block-scope, class-name, read-before-declaration
// questions: §67
// tsc: rejects TS2351, TS2448, TS2454
// expected-error: S100 at the Foo constructor read

class Foo {
  value: i32;

  constructor() {
    this.value = 1;
  }
}

export function main(): void {
  const item: Foo = new Foo();
  const Foo: i32 = 9;
  print(`${item.value}:${Foo}`);
}
