// corpus: accept/a70-json-roundtrip
// purpose: Round-trips every P13 serializable family except untagged Date.
// exercises: JSON, typed parse, stringify, JsonResult, static type validation
// questions: Q28, Q6, Q14, Q5
// tsc: accepts
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
  const i8Result: JsonResult<i8> = JSON.parse(JSON.stringify(i8Value));
  if (i8Result.ok) print(JSON.stringify(i8Result.value));
  Context.free(i8Result);

  const u8Value: u8 = 250;
  const u8Result: JsonResult<u8> = JSON.parse(JSON.stringify(u8Value));
  if (u8Result.ok) print(JSON.stringify(u8Result.value));
  Context.free(u8Result);

  const i16Value: i16 = -1600;
  const i16Result: JsonResult<i16> = JSON.parse(JSON.stringify(i16Value));
  if (i16Result.ok) print(JSON.stringify(i16Result.value));
  Context.free(i16Result);

  const u16Value: u16 = 65000;
  const u16Result: JsonResult<u16> = JSON.parse(JSON.stringify(u16Value));
  if (u16Result.ok) print(JSON.stringify(u16Result.value));
  Context.free(u16Result);

  const i32Value: i32 = -2000000000;
  const i32Result: JsonResult<i32> = JSON.parse(JSON.stringify(i32Value));
  if (i32Result.ok) print(JSON.stringify(i32Result.value));
  Context.free(i32Result);

  const u32Value: u32 = 4000000000;
  const u32Result: JsonResult<u32> = JSON.parse(JSON.stringify(u32Value));
  if (u32Result.ok) print(JSON.stringify(u32Result.value));
  Context.free(u32Result);

  const i64Value: i64 = -9007199254740991;
  const i64Result: JsonResult<i64> = JSON.parse(JSON.stringify(i64Value));
  if (i64Result.ok) print(JSON.stringify(i64Result.value));
  Context.free(i64Result);

  const u64Value: u64 = 9007199254740991;
  const u64Result: JsonResult<u64> = JSON.parse(JSON.stringify(u64Value));
  if (u64Result.ok) print(JSON.stringify(u64Result.value));
  Context.free(u64Result);

  const f32Value: f32 = 1.5;
  const f32Result: JsonResult<f32> = JSON.parse(JSON.stringify(f32Value));
  if (f32Result.ok) print(JSON.stringify(f32Result.value));
  Context.free(f32Result);

  const f64Value: f64 = 0.000001;
  const f64Result: JsonResult<f64> = JSON.parse(JSON.stringify(f64Value));
  if (f64Result.ok) print(JSON.stringify(f64Result.value));
  Context.free(f64Result);

  const negativeZero: f64 = -0.0;
  const negativeZeroResult: JsonResult<f64> =
    JSON.parse(JSON.stringify(negativeZero));
  if (negativeZeroResult.ok) print(JSON.stringify(negativeZeroResult.value));
  Context.free(negativeZeroResult);

  const trueResult: JsonResult<boolean> = JSON.parse(JSON.stringify(true));
  if (trueResult.ok) print(JSON.stringify(trueResult.value));
  Context.free(trueResult);

  const falseResult: JsonResult<boolean> = JSON.parse(JSON.stringify(false));
  if (falseResult.ok) print(JSON.stringify(falseResult.value));
  Context.free(falseResult);

  const escaped: string =
    "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\u0008\u0009\u000a\u000b\u000c\u000d\u000e\u000f" +
    "\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f" +
    " \"/\\\u007f\u0080\u2028\u2029";
  const stringResult: JsonResult<string> = JSON.parse(JSON.stringify(escaped));
  if (stringResult.ok) print(JSON.stringify(stringResult.value));
  Context.free(stringResult);

  const nested: i32[][] = [[1, 2], [], [-3, 4]];
  const nestedResult: JsonResult<i32[][]> = JSON.parse(JSON.stringify(nested));
  if (nestedResult.ok) print(JSON.stringify(nestedResult.value));
  Context.free(nestedResult);

  const fixed: FixedArray<i16, 3> = [-2, 0, 7];
  const fixedResult: JsonResult<FixedArray<i16, 3>> =
    JSON.parse(JSON.stringify(fixed));
  if (fixedResult.ok) print(JSON.stringify(fixedResult.value));
  Context.free(fixedResult);

  const point: Point = new Point(9, true);
  const pointResult: JsonResult<Point> = JSON.parse(JSON.stringify(point));
  if (pointResult.ok) print(JSON.stringify(pointResult.value));
  Context.free(pointResult);

  const person: Person = new Person("Ada", 37, false);
  const personResult: JsonResult<Person> = JSON.parse(JSON.stringify(person));
  if (personResult.ok) print(JSON.stringify(personResult.value));
  Context.free(personResult);

  let maybe: Person | null = person;
  const someResult: JsonResult<Person | null> = JSON.parse(JSON.stringify(maybe));
  if (someResult.ok) print(JSON.stringify(someResult.value));
  Context.free(someResult);

  maybe = null;
  const nullResult: JsonResult<Person | null> = JSON.parse(JSON.stringify(maybe));
  if (nullResult.ok) print(JSON.stringify(nullResult.value));
  Context.free(nullResult);

  // Date is intentionally absent: JSON.stringify(Date) produces an
  // untagged string, so JSON.parse<Date> cannot distinguish it from data.
  Context.collect();
}
