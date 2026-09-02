// corpus: accept/a178-generic-method
// purpose: Runs generic instance and static methods at explicit type arguments.
// observable: Each type-argument list prints its own result, so one template serves many types.
// exercises: generic-method, static-generic-method, monomorphization, value-class-receiver
// questions: R39.6, §82.4, §64
// tsc: accepts; js-comparable: no C2 C8: The CStruct decorator has no JavaScript shim.
@CStruct
class Vec2 {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }

  pick<T>(value: T): T {
    return value;
  }
}

function firstOf<T>(values: T[]): T {
  return values[0];
}

class Box {
  v: i32;

  constructor(v: i32) {
    this.v = v;
  }

  identity<T>(value: T): T {
    return value;
  }

  join<A, B>(left: A, right: B): string {
    return `${left}/${right}`;
  }

  head<T>(values: T[]): T {
    return firstOf<T>(values);
  }

  static create<T>(values: T[]): Box {
    return new Box(values.length);
  }
}

export function main(): void {
  const box: Box = new Box(1);
  print(`i32:${box.identity<i32>(3)}`);
  print(`i32-again:${box.identity<i32>(4)}`);
  print(`string:${box.identity<string>("head")}`);

  const vector: Vec2 = box.identity<Vec2>(new Vec2(1.5, 2.5));
  print(`vec:${vector.x},${vector.y}`);

  print(`join:${box.join<i32, string>(5, "tail")}`);
  print(`head:${box.head<i32>([7, 8])}`);
  print(`head-string:${box.head<string>(["alpha", "beta"])}`);

  const made: Box = Box.create<string>(["a", "b", "c"]);
  print(`created:${made.v}`);

  const point: Vec2 = new Vec2(3.5, 4.5);
  print(`pick:${point.pick<i32>(9)}`);
  print(`pick-string:${point.pick<string>("edge")}`);
}
