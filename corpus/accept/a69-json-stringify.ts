// corpus: accept/a69-json-stringify
// purpose: Serializes every P13 stage-1 JSON.stringify input family.
// exercises: JSON, scalars, UTF-8 escaping, Date, arrays, value/reference classes, nullable
// questions: Q28, Q14, Q5, Q20

@CStruct
class Point {
  x: i32;
  ready: boolean;

  constructor(x: i32, ready: boolean) {
    this.x = x;
    this.ready = ready;
  }
}

class Person {
  name: string;
  age: i32;
  active: boolean;

  constructor(name: string, age: i32, active: boolean) {
    this.name = name;
    this.age = age;
    this.active = active;
  }
}

export function main(): void {
  const i8Value: i8 = -8;
  const u8Value: u8 = 250;
  const i16Value: i16 = -1600;
  const u16Value: u16 = 65000;
  const i32Value: i32 = -2000000000;
  const u32Value: u32 = 4000000000;
  const i64Value: i64 = -9007199254740991;
  const u64Value: u64 = 9007199254740991;
  const f32Value: f32 = 1.5;
  const f64Value: f64 = 0.000001;
  print(JSON.stringify(i8Value));
  print(JSON.stringify(u8Value));
  print(JSON.stringify(i16Value));
  print(JSON.stringify(u16Value));
  print(JSON.stringify(i32Value));
  print(JSON.stringify(u32Value));
  print(JSON.stringify(i64Value));
  print(JSON.stringify(u64Value));
  print(JSON.stringify(f32Value));
  print(JSON.stringify(f64Value));
  print(JSON.stringify(-0.0));
  print(JSON.stringify(true));
  print(JSON.stringify(false));

  const escaped: string =
    "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\u0008\u0009\u000a\u000b\u000c\u000d\u000e\u000f" +
    "\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f" +
    " \"/\\\u007f\u0080\u2028\u2029";
  print(JSON.stringify(escaped));

  const epoch: Date = new Date(0);
  print(JSON.stringify(epoch));

  const nested: i32[][] = [[1, 2], [], [-3, 4]];
  print(JSON.stringify(nested));

  const fixed: FixedArray<i16, 3> = [-2, 0, 7];
  print(JSON.stringify(fixed));

  const point: Point = new Point(9, true);
  print(JSON.stringify(point));

  const person: Person = new Person("Ada", 37, false);
  print(JSON.stringify(person));

  let maybe: Person | null = person;
  print(JSON.stringify(maybe));
  maybe = null;
  print(JSON.stringify(maybe));
}
