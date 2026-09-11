// corpus: accept/a231-constructor-assigns-every-field
// purpose: Accepts a class whose constructor assigns every field at its top level.
// observable: each field prints the value the constructor stored.
// exercises: class-field, constructor, value-class, reference-field
// questions: compiler section 108
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct
class Vec2 {
  x: i32;
  y: i32;

  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

class Inner {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

class Holder {
  count: i32;
  label: string;
  inner: Inner;
  origin: Vec2;

  constructor(count: i32, label: string) {
    this.count = count;
    this.label = label;
    this.inner = new Inner(count * 2);
    this.origin = new Vec2(count + 1, count + 2);
  }
}

export function main(): void {
  const holder: Holder = new Holder(3, "set");
  print(`${holder.count} ${holder.label} ${holder.inner.value}`);
  print(`${holder.origin.x} ${holder.origin.y}`);
}
