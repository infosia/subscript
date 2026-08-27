// corpus: accept/a142-bytes-of
// purpose: Proves byte copies for aligned value classes and fixed arrays.
// exercises: Context.bytesOf, Context.bytesInto, Context.fromBytes, padding-zero
// questions: R34
// tsc: accepts
@CStruct({ align: 16 })
class Vec3f {
  x: f32 = 0.0;
  y: f32 = 0.0;
  z: f32 = 0.0;

  constructor(x: f32, y: f32, z: f32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}

export function main(): void {
  const array: FixedArray<Vec3f, 2> = [
    new Vec3f(1.0, 2.0, 3.0),
    new Vec3f(4.0, 5.0, 6.0),
  ];
  const arrayBytes: u8[] = Context.bytesOf<FixedArray<Vec3f, 2>>(array);
  print(`${arrayBytes.length}`);

  const elementBytes: u8[] = Context.bytesOf<Vec3f>(array[0]);
  print(elementBytes.join(","));

  const target: u8[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  Context.bytesInto<Vec3f>(array[0], target, 4);
  print(`${target[6]}`);

  const decoded: Vec3f = Context.fromBytes<Vec3f>(target, 4);
  print(`${decoded.x},${decoded.y},${decoded.z}`);
}
