// corpus: accept/a47-narrow-layout
// purpose: Stores narrow and established numeric fields in one C-layout value class.
// exercises: narrow-numerics, value-struct, mixed-field-layout, copy-on-assign
// questions: Q2, Q23, C2, C3, C4

@CStruct
class NarrowRecord {
  kind: u8;
  delta: i16;
  weight: f16;
  serial: u64;
  bias: i8;
  count: u16;
  scale: f32;

  constructor(
    kind: u8,
    delta: i16,
    weight: f16,
    serial: u64,
    bias: i8,
    count: u16,
    scale: f32,
  ) {
    this.kind = kind;
    this.delta = delta;
    this.weight = weight;
    this.serial = serial;
    this.bias = bias;
    this.count = count;
    this.scale = scale;
  }
}

export function main(): void {
  const original: NarrowRecord = new NarrowRecord(7, -300, 1.5, 99, -4, 1000, 2.25);
  const copy: NarrowRecord = original;
  copy.kind = 9;
  print(
    `${original.kind} ${copy.kind} ${copy.delta} ${copy.weight as f32} ${copy.serial} ${copy.bias} ${copy.count} ${copy.scale}`,
  );
}
