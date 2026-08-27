// corpus: accept/a76-p20-dynamic-value-field-write
// purpose: Writes fields through a value-class element of a dynamic array.
// exercises: Array<CStruct>, field-place, plain-assignment, compound-assignment, side-effecting-index
// questions: none
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct
class Vec2 {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

let indexCalls: i32 = 0;

function index(): i32 {
  indexCalls += 1;
  return 1;
}

export function fault(): void {
  const values: Vec2[] = [new Vec2(1, 2), new Vec2(3, 4)];
  const zero: i32 = 0;
  values[index()].x /= zero;
}

export function main(): void {
  const values: Vec2[] = [new Vec2(1, 2), new Vec2(3, 4)];
  values[1].x = 9;
  values[index()].x += 5;
  print(`${values[1].x},${indexCalls}`);
}
