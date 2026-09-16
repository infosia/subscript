// corpus: reject/r234-sandbox-from-bytes
// profile: sandbox
// purpose: Rejects Context.fromBytes under the sandbox profile; forged bytes have no layout proof.
// exercises: sandbox-profile, Context.fromBytes
// questions: R34
// tsc: accepts
// expected-error: S024 at `Context.fromBytes`
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
