// corpus: accept/a302-generic-instance-globals
// purpose: A generic instance body resolves a later module global.
// observable: The instance method prints 7 after module initialization.
// exercises: generic-class, module-global, instance-method
// questions: compiler.md §138
// tsc: accepts; js-comparable: yes
class Foo { v: i32 = 7; }
class Box<T> {
  x: T;
  constructor(x: T) { this.x = x; }
  peek(): i32 { return m.v; }
}
const b: Box<i32> = new Box<i32>(1);
const m: Foo = new Foo();
export function main(): void { print(`${b.peek()}`); }
