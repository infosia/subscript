// corpus: accept/a141-cstruct-align
// purpose: Proves explicit value-class alignment in nested classes and fixed arrays.
// exercises: CStruct-alignment, value-copy, nested-value-class, FixedArray-stride
// questions: R33
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
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

@CStruct
class Mixed {
  a: f32 = 0.0;
  p: Vec3f = new Vec3f(0.0, 0.0, 0.0);

  constructor(a: f32, p: Vec3f) {
    this.a = a;
    this.p = p;
  }
}

@CStruct
class Vec3Buffer {
  values: FixedArray<Vec3f, 4> = [
    new Vec3f(0.0, 0.0, 0.0),
    new Vec3f(0.0, 0.0, 0.0),
    new Vec3f(0.0, 0.0, 0.0),
    new Vec3f(0.0, 0.0, 0.0),
  ];
}

export function main(): void {
  const source: Vec3f = new Vec3f(1.0, 2.0, 3.0);
  const copied: Vec3f = source;
  copied.x = 9.0;
  const mixed: Mixed = new Mixed(4.0, new Vec3f(5.0, 6.0, 7.0));
  const buffer: Vec3Buffer = new Vec3Buffer();
  buffer.values[2] = new Vec3f(8.0, 9.0, 10.0);

  print(`source=${source.x},${source.y},${source.z}`);
  print(`copied=${copied.x},${copied.y},${copied.z}`);
  print(`mixed=${mixed.a},${mixed.p.x},${mixed.p.y},${mixed.p.z}`);
  print(`array=${buffer.values[2].x},${buffer.values[2].y},${buffer.values[2].z}`);
}
