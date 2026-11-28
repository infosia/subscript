// corpus: accept/a513-scalar-bytes
// purpose: Proves byte copies of each scalar type, and that they equal the bytes of the one-element FixedArray form.
// exercises: Context.bytesOf, Context.bytesInto, Context.fromBytes, scalar-bytes, generic-instance
// questions: R34, compiler.md §360
// tsc: accepts; js-comparable: no Q3: The Context byte API has no JavaScript shim.
enum Mode {
  Off,
  On = 300,
}

function report<T>(name: string, value: T): void {
  const bytes: u8[] = Context.bytesOf<T>(value);
  const back: T = Context.fromBytes<T>(bytes, 0);
  const target: u8[] = [9, 9, 9, 9, 9, 9, 9, 9, 9, 9];
  Context.bytesInto<T>(value, target, 1);
  const wrapped: FixedArray<T, 1> = [value];
  const same: boolean = bytes.join(",") === Context.bytesOf<FixedArray<T, 1>>(wrapped).join(",");
  print(`${name} ${bytes.length} [${bytes.join(",")}] ${back} [${target.join(",")}] same=${same}`);
}

export function main(): void {
  report<i8>("i8", -5);
  report<u8>("u8", 200);
  report<i16>("i16", -300);
  report<u16>("u16", 65000);
  report<i32>("i32", -2);
  report<u32>("u32", 4000000000);
  report<i64>("i64", -9007199254740993);
  report<u64>("u64", 9007199254740993);
  report<f16>("f16", 1.5);
  report<f32>("f32", -0.25);
  report<f64>("f64", 1.0e300);
  report<boolean>("boolean", true);
  report<Mode>("Mode", Mode.On);

  // A negative i32 in a direct call.
  const negative: i32 = -2;
  print(Context.bytesOf<i32>(negative).join(","));

  // An f32 NaN with a payload keeps its bytes.
  const nanBytes: u8[] = [1, 0, 192, 127];
  const nan: f32 = Context.fromBytes<f32>(nanBytes, 0);
  const nanArray: FixedArray<f32, 1> = Context.fromBytes<FixedArray<f32, 1>>(nanBytes, 0);
  print(`${nan} ${Context.bytesOf<f32>(nan).join(",")} ${Context.bytesOf<FixedArray<f32, 1>>(nanArray).join(",")}`);

  // A u64 above 2^53.
  const big: u64 = 9007199254740993;
  print(`${big} ${Context.bytesOf<u64>(big).join(",")}`);

  // A u32 read at an offset that is not a multiple of 4.
  const eight: u8[] = [1, 2, 3, 4, 5, 6, 7, 8];
  const odd: u32 = Context.fromBytes<u32>(eight, 3);
  print(`${odd} ${Context.fromBytes<FixedArray<u32, 1>>(eight, 3)[0]}`);

  // A boolean byte 2 reads as true and writes back as 1.
  const two: u8[] = [2];
  const flag: boolean = Context.fromBytes<boolean>(two, 0);
  const flagArray: FixedArray<boolean, 1> = Context.fromBytes<FixedArray<boolean, 1>>(two, 0);
  print(`${flag} ${Context.bytesOf<boolean>(flag).join(",")} ${flagArray[0]} ${Context.bytesOf<FixedArray<boolean, 1>>(flagArray).join(",")}`);
  const off: boolean = Context.fromBytes<boolean>([0], 0);
  print(`${off} ${Context.bytesOf<boolean>(off).join(",")}`);

  // An enum round trip.
  const modeBytes: u8[] = Context.bytesOf<Mode>(Mode.On);
  const mode: Mode = Context.fromBytes<Mode>(modeBytes, 0);
  print(`${modeBytes.join(",")} ${mode === Mode.On} ${Context.fromBytes<Mode>([0, 0, 0, 0], 0) === Mode.Off}`);

  // A literal argument and an expression argument.
  print(Context.bytesOf<u32>(25).join(","));
  const a: u32 = 41;
  print(Context.bytesOf<u32>(a + 1).join(","));
  const into: u8[] = [0, 0, 0, 0, 0, 0];
  const b: u16 = 7;
  Context.bytesInto<u16>(b * 3, into, 3);
  print(into.join(","));
}
