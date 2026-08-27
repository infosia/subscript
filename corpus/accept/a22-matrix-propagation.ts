// corpus: accept/a22-matrix-propagation
// purpose: Runs the fixed matrix-propagation benchmark and prints one f32 checksum.
// exercises: value-struct, fixed-array, slices, lcg, matrix-propagation, benchmark
// questions: Q1, Q2, Q3, Q4, Q12, Q14, Q15, Q17
// tsc: accepts
const NODE_COUNT: i32 = 10000;
const ITERATION_COUNT: i32 = 100;
let lcgState: u32 = 0x12345678;

@CStruct
class Matrix4 {
  elements: FixedArray<f32, 16>;

  constructor(elements: FixedArray<f32, 16>) {
    this.elements = elements;
  }
}

function nextU32(): u32 {
  lcgState = (lcgState * 1664525 + 1013904223) as u32;
  return lcgState;
}

function nextF32(): f32 {
  return (nextU32() as f32) / (4294967296.0 as f32);
}

function identityWithTranslation(x: f32, y: f32, z: f32): Matrix4 {
  return new Matrix4([
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    x, y, z, 1.0,
  ]);
}

function makeLocalMatrix(): Matrix4 {
  const x: f32 = (nextF32() - 0.5) * 0.01;
  const y: f32 = (nextF32() - 0.5) * 0.01;
  const z: f32 = (nextF32() - 0.5) * 0.01;
  return identityWithTranslation(x, y, z);
}

function multiply(left: Matrix4, right: Matrix4): Matrix4 {
  const result: FixedArray<f32, 16> = [
    0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0,
  ];

  for (let row: i32 = 0; row < 4; row += 1) {
    for (let column: i32 = 0; column < 4; column += 1) {
      let cell: f32 = 0.0;
      for (let inner: i32 = 0; inner < 4; inner += 1) {
        cell += left.elements[row * 4 + inner] * right.elements[inner * 4 + column];
      }
      result[row * 4 + column] = cell;
    }
  }

  return new Matrix4(result);
}

function perturbLocals(local: Matrix4[], iteration: i32): void {
  for (let index: i32 = 0; index < local.length; index += 1) {
    const matrix: Matrix4 = local[index];
    const phase: i32 = (iteration + index) % 17;
    const delta: f32 = ((phase as f32) - 8.0) * 0.000001;
    matrix.elements[12] += delta;
    matrix.elements[13] -= delta * 0.5;
    local[index] = matrix;
  }
}

function propagate(local: Matrix4[], world: Matrix4[], parent: i32[]): void {
  world[0] = local[0];
  for (let index: i32 = 1; index < local.length; index += 1) {
    world[index] = multiply(world[parent[index]], local[index]);
  }
}

function checksum(matrices: Matrix4[]): f32 {
  let total: f32 = 0.0;
  for (let matrixIndex: i32 = 0; matrixIndex < matrices.length; matrixIndex += 1) {
    for (let elementIndex: i32 = 0; elementIndex < 16; elementIndex += 1) {
      total += matrices[matrixIndex].elements[elementIndex];
    }
  }
  return total;
}

export function main(): void {
  const local: Matrix4[] = [];
  const world: Matrix4[] = [];
  const parent: i32[] = [];

  for (let index: i32 = 0; index < NODE_COUNT; index += 1) {
    local.push(makeLocalMatrix());
    world.push(identityWithTranslation(0.0, 0.0, 0.0));
    if (index === 0) {
      parent.push(-1);
    } else {
      parent.push((nextU32() % (index as u32)) as i32);
    }
  }

  for (let iteration: i32 = 0; iteration < ITERATION_COUNT; iteration += 1) {
    perturbLocals(local, iteration);
    propagate(local, world, parent);
  }

  const result: f32 = checksum(world);
  print(`${result}`);
}
