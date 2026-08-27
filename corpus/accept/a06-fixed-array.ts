// corpus: accept/a06-fixed-array
// purpose: Places a fixed-size array field inside a value struct.
// exercises: value-struct, fixed-array, field-layout
// questions: Q1, Q2, Q3, Q12
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct
class Matrix4 {
  elements: FixedArray<f32, 16>;

  constructor(elements: FixedArray<f32, 16>) {
    this.elements = elements;
  }
}

export function main(): void {
  const matrix: Matrix4 = new Matrix4([
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    4.0, 5.0, 6.0, 1.0,
  ]);
  print(`${matrix.elements[12]},${matrix.elements[13]},${matrix.elements[14]}`);
}
