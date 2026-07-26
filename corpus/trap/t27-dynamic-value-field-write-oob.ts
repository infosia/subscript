// corpus: trap/t27-dynamic-value-field-write-oob
// purpose: Traps while resolving a field place through a value-class element of a dynamic array.
// exercises: Array<CStruct>, field-place, compound-assignment, side-effecting-index, index-read
// questions: none
// expected-trap: index-out-of-bounds at values[index()].x

@CStruct
class Vec2 {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

function index(): i32 {
  print("index");
  return 3;
}

export function main(): void {
  const values: Vec2[] = [new Vec2(1, 2), new Vec2(3, 4)];
  const zero: i32 = 0;
  values[index()].x /= zero;
}
