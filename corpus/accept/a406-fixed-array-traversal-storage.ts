// corpus: accept/a406-fixed-array-traversal-storage
// purpose: A traversal reads storage, so a whole assignment to the place gives later steps the new elements, and an element of a dynamic array keeps its index. Each copy that C2 and C5 name stays a copy.
// exercises: for-of, fixed-array, forEach, reduceRight, value-class, nested-field, this, parameter, closure-capture, shift
// questions: compiler.md §220.1 rules 4, 5, 6, 9
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim, an assignment to a FixedArray copies into its storage, a dynamic-array element subject keeps its index, and a pass copies.
@ValueType
class Vec {
  a: FixedArray<i32, 2> = [1, 2];

  walk(): i32 {
    let total: i32 = 0;
    for (const x of this.a) {
      total = total * 10 + x;
      this.a[1] = 9;
    }
    return total;
  }

  walkEach(): i32 {
    return this.a.reduce((acc: i32, x: i32): i32 => acc * 10 + x, 0);
  }
}

class Holder {
  v: Vec = new Vec();
  a: FixedArray<i32, 2> = [1, 2];

  walk(): void {
    const seen: i32[] = [];
    for (const x of this.a) {
      seen.push(x);
      this.a[1] = 9;
    }
    line("this-field", seen);
  }
}

function line(label: string, values: i32[]): void {
  print(`${label} ${values.join(",")}`);
}

function wholeLocal(): void {
  let b: FixedArray<i32, 2> = [1, 2];
  const c: FixedArray<i32, 2> = [7, 8];
  const seen: i32[] = [];
  for (const x of b) {
    seen.push(x);
    b = c;
  }
  line("whole-local", seen);
}

function wholeField(): void {
  const r = new Holder();
  const c: FixedArray<i32, 2> = [7, 8];
  const seen: i32[] = [];
  for (const x of r.a) {
    seen.push(x);
    r.a = c;
  }
  line("whole-field", seen);
}

function shifted(): void {
  const m: FixedArray<i32, 2>[] = [[1, 2], [3, 4]];
  const seen: i32[] = [];
  for (const x of m[0]) {
    seen.push(x);
    m.shift();
  }
  line("shifted", seen);
}

function valueLocal(): void {
  const v = new Vec();
  const seen: i32[] = [];
  for (const x of v.a) {
    seen.push(x);
    v.a[1] = 9;
  }
  line("value-local", seen);
}

function valueNested(): void {
  const r = new Holder();
  const seen: i32[] = [];
  for (const x of r.v.a) {
    seen.push(x);
    r.v.a[1] = 9;
  }
  line("value-nested", seen);
}

function valueCallback(): void {
  const r = new Holder();
  const seen: i32[] = [];
  r.v.a.forEach((x: i32): void => {
    seen.push(x);
    r.v.a[1] = 9;
  });
  line("value-callback", seen);
}

function thisForms(): void {
  const v = new Vec();
  line("this-value", [v.walk(), v.walkEach()]);
  new Holder().walk();
}

function rowBinding(): void {
  const grid: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  const seen: i32[] = [];
  for (const row of grid) {
    row[1] = 9;
    seen.push(row[1]);
  }
  line("row-copy", seen);
  line("row-binding", [grid[0][1], grid[1][1]]);
}

function write(p: FixedArray<i32, 2>): i32 {
  p[1] = 9;
  return p[1];
}

function passCopy(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  const seen: i32[] = [];
  for (const x of b) {
    seen.push(x);
    seen.push(write(b));
  }
  line("pass", seen);
}

function parameter(p: FixedArray<i32, 2>): void {
  const seen: i32[] = [];
  for (const x of p) {
    seen.push(x);
    p[1] = 9;
  }
  line("parameter", seen);
}

function* walkCopy(p: FixedArray<i32, 2>): Generator<i32> {
  for (const x of p) {
    yield x;
  }
}

function generatorCopy(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  const it = walkCopy(b);
  b[1] = 9;
  const seen: i32[] = [];
  for (const x of it) {
    seen.push(x);
  }
  line("generator", seen);
}

function capturedCopy(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  const seen: i32[] = [];
  const f = (x: i32): void => {
    seen.push(x + b[1]);
  };
  b[1] = 9;
  b.forEach(f);
  line("captured", seen);
  const walk = (): i32 => {
    let total: i32 = 0;
    for (const x of b) {
      total = total * 10 + x;
    }
    return b.reduce((acc: i32, x: i32): i32 => acc * 10 + x, total);
  };
  line("captured-walk", [walk()]);
}

export function main(): void {
  wholeLocal();
  wholeField();
  shifted();
  valueLocal();
  valueNested();
  valueCallback();
  thisForms();
  rowBinding();
  passCopy();
  const b: FixedArray<i32, 2> = [1, 2];
  parameter(b);
  line("parameter-caller", [b[1]]);
  generatorCopy();
  capturedCopy();
}
