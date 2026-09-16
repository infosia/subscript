// corpus: accept/a236-sandbox-from-bytes-default
// purpose: Runs the r234 source under the default profile; Context.fromBytes stays callable there.
// exercises: sandbox-profile-twin, Context.fromBytes
// questions: R34
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct({ align: 4 })
class Point {
  x: i32 = 0;
  y: i32 = 0;
}

export function main(): void {
  const bytes: u8[] = [1, 0, 0, 0, 2, 0, 0, 0];
  const point: Point = Context.fromBytes<Point>(bytes, 0);
  print(`${point.x},${point.y}`);
}
