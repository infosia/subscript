// corpus: accept/a04-value-struct
// purpose: Makes value-struct copy-on-assign semantics observable.
// exercises: value-struct, field-access, copy-on-assign
// questions: Q1, Q2, Q12, Q14, Q17

@value
class Vec3 {
  x: f32;
  y: f32;
  z: f32;

  constructor(x: f32, y: f32, z: f32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}

export function main(): void {
  const original: Vec3 = new Vec3(1.0, 2.0, 3.0);
  const copy: Vec3 = original;
  copy.x = 9.0;
  print(`${original.x},${copy.x}`);
}
