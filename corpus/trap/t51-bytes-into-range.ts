// corpus: trap/t51-bytes-into-range
// purpose: A byte copy traps when its complete value storage does not fit.
// exercises: Context.bytesInto, byte-range, trap-stop
// questions: R34
// tier-policy: both tiers trap with kind 1
// expected-trap: byte range at offset 5 with size 16 exceeds array length 20

@CStruct({ align: 16 })
class Vec3f {
  x: f32 = 0.0;
  y: f32 = 0.0;
  z: f32 = 0.0;
}

export function main(): void {
  const value: Vec3f = new Vec3f();
  const target: u8[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  Context.bytesInto<Vec3f>(value, target, 5);
}
