// corpus: accept/a144-accessor
// purpose: Uses named accessors as read and write method sugar.
// exercises: read-accessor, write-accessor, value-class-accessor, generic-accessor, identifier-escape, sanitized-name-collision
// questions: R37

class Counter {
  private value: i32;

  constructor(value: i32) {
    this.value = value;
  }

  get current(): i32 {
    return this.value;
  }

  set current(value: i32) {
    this.value = value;
  }

  get doubled(): i32 {
    return this.value * 2;
  }
}

@CStruct
class Point {
  x: i32;

  constructor(x: i32) {
    this.x = x;
  }

  get coordinate(): i32 {
    return this.x;
  }
}

class Box<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }

  get item(): T {
    return this.value;
  }
}

class EscapedNames {
  get $(): i32 {
    return 1;
  }

  _(): i32 {
    return 2;
  }
}

class SetterNameCollision {
  private value: i32 = 1;

  get v(): i32 {
    return this.value;
  }

  set v(value: i32) {
    this.value = value;
  }

  v_set_(): i32 {
    return 300;
  }
}

export function main(): void {
  const counter: Counter = new Counter(3);
  print(`${counter.current}`);
  counter.current = 8;
  print(`${counter.current}`);
  print(`current=${counter.current}`);
  print(`${counter.doubled}`);

  const point: Point = new Point(5);
  print(`${point.coordinate}`);

  const integerBox: Box<i32> = new Box<i32>(11);
  const stringBox: Box<string> = new Box<string>("hello");
  print(`${integerBox.item}`);
  print(stringBox.item);

  const escaped: EscapedNames = new EscapedNames();
  print(`${escaped.$}`);
  print(`${escaped._()}`);

  const collision: SetterNameCollision = new SetterNameCollision();
  collision.v = 2;
  print(`${collision.v}`);
  print(`${collision.v_set_()}`);
}
