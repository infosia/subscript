// corpus: accept/a70-json-roundtrip
// purpose: Round-trips every P13 serializable family except untagged Date.
// exercises: JSON, typed parse, stringify, direct result, static type validation
// questions: Q28, Q6, Q14, Q5
// tsc: accepts; js-comparable: no C2 Q6 Q7 Q28: The CStruct decorator has no JavaScript shim.
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
  const i8Result: i8 = JSON.parse(JSON.stringify(i8Value));
  print(JSON.stringify(i8Result));

  const u8Value: u8 = 250;
  const u8Result: u8 = JSON.parse(JSON.stringify(u8Value));
  print(JSON.stringify(u8Result));

  const i16Value: i16 = -1600;
  const i16Result: i16 = JSON.parse(JSON.stringify(i16Value));
  print(JSON.stringify(i16Result));

  const u16Value: u16 = 65000;
  const u16Result: u16 = JSON.parse(JSON.stringify(u16Value));
  print(JSON.stringify(u16Result));

  const i32Value: i32 = -2000000000;
  const i32Result: i32 = JSON.parse(JSON.stringify(i32Value));
  print(JSON.stringify(i32Result));

  const u32Value: u32 = 4000000000;
  const u32Result: u32 = JSON.parse(JSON.stringify(u32Value));
  print(JSON.stringify(u32Result));

  const i64Value: i64 = -9007199254740991;
  const i64Result: i64 = JSON.parse(JSON.stringify(i64Value));
  print(JSON.stringify(i64Result));

  const u64Value: u64 = 9007199254740991;
  const u64Result: u64 = JSON.parse(JSON.stringify(u64Value));
  print(JSON.stringify(u64Result));

  const f32Value: f32 = 1.5;
  const f32Result: f32 = JSON.parse(JSON.stringify(f32Value));
  print(JSON.stringify(f32Result));

  const f64Value: f64 = 0.000001;
  const f64Result: f64 = JSON.parse(JSON.stringify(f64Value));
  print(JSON.stringify(f64Result));

  const negativeZero: f64 = -0.0;
  const negativeZeroResult: f64 =
    JSON.parse(JSON.stringify(negativeZero));
  print(JSON.stringify(negativeZeroResult));

  const trueResult: boolean = JSON.parse(JSON.stringify(true));
  print(JSON.stringify(trueResult));

  const falseResult: boolean = JSON.parse(JSON.stringify(false));
  print(JSON.stringify(falseResult));

  const escaped: string =
    "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\u0008\u0009\u000a\u000b\u000c\u000d\u000e\u000f" +
    "\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f" +
    " \"/\\\u007f\u0080\u2028\u2029";
  const stringResult: string = JSON.parse(JSON.stringify(escaped));
  print(JSON.stringify(stringResult));

  const nested: i32[][] = [[1, 2], [], [-3, 4]];
  const nestedResult: i32[][] = JSON.parse(JSON.stringify(nested));
  print(JSON.stringify(nestedResult));

  const fixed: FixedArray<i16, 3> = [-2, 0, 7];
  const fixedResult: FixedArray<i16, 3> =
    JSON.parse(JSON.stringify(fixed));
  print(JSON.stringify(fixedResult));

  const point: Point = new Point(9, true);
  const pointResult: Point = JSON.parse(JSON.stringify(point));
  print(JSON.stringify(pointResult));

  const person: Person = new Person("Ada", 37, false);
  const personResult: Person = JSON.parse(JSON.stringify(person));
  print(JSON.stringify(personResult));

  let maybe: Person | null = person;
  const someResult: Person | null = JSON.parse(JSON.stringify(maybe));
  print(JSON.stringify(someResult));

  maybe = null;
  const nullResult: Person | null = JSON.parse(JSON.stringify(maybe));
  print(JSON.stringify(nullResult));

  // Date is intentionally absent: JSON.stringify(Date) produces an
  // untagged string, so JSON.parse<Date> cannot distinguish it from data.
  Context.collect();
}
