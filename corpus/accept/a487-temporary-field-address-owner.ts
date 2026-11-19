// corpus: accept/a487-temporary-field-address-owner
// purpose: A field address keeps the temporary object alive during a method collection.
// observable: The value field keeps its original value.
// exercises: value-type, field-address, temporary, collect
// questions: compiler.md §338 rule 10, §323
// compiler.md §338 rule 10: three-tier debug best of three: 0.25 s; 1 C build.
// tsc: accepts
// js-comparable: no C2: Value types use inline C storage.
@ValueType
class Value {
  x: i32;
  constructor(x: i32) { this.x = x; }
  read(): i32 {
    Context.collect();
    for (let i: i32 = 0; i < 8; i += 1) {
      held.push(new Box(new Value(9)));
    }
    return this.x;
  }
}
class Box {
  value: Value;
  constructor(value: Value) { this.value = value; }
}
let held: Box[] = [];
export function main(): void {
  print(`${new Box(new Value(1)).value.read()}`);
}
