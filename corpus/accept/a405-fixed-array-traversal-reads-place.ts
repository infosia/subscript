// corpus: accept/a405-fixed-array-traversal-reads-place
// purpose: A for-of and each callback method over a FixedArray place read the place at each step, and evaluate the place one time.
// exercises: for-of, fixed-array, forEach, map, filter, some, every, findIndex, reduce, reduceRight, global, field, nested-field, element-place, reference-class
// questions: compiler.md §220
// tsc: accepts; js-comparable: yes
class Inner {
  a: FixedArray<i32, 2> = [1, 2];
}

class Holder {
  a: FixedArray<i32, 2> = [1, 2];
  v: Inner = new Inner();
  m: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
}

let g: FixedArray<i32, 2> = [1, 2];

function writeGlobal(): void {
  g[1] = 9;
}

function line(label: string, values: i32[]): void {
  print(`${label} ${values.join(",")}`);
}

function localSubject(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  const seen: i32[] = [];
  for (const x of b) {
    seen.push(x);
    b[1] = 9;
  }
  line("local", seen);
}

function globalSubject(): void {
  const seen: i32[] = [];
  for (const x of g) {
    seen.push(x);
    writeGlobal();
  }
  line("global", seen);
  g[1] = 2;
}

function fieldSubject(): void {
  const r = new Holder();
  const seen: i32[] = [];
  for (const x of r.a) {
    seen.push(x);
    r.a[1] = 9;
  }
  line("field", seen);
}

function nestedFieldSubject(): void {
  const r = new Holder();
  const seen: i32[] = [];
  for (const x of r.v.a) {
    seen.push(x);
    r.v.a[1] = 9;
  }
  line("nested", seen);
}

function fixedElementSubject(): void {
  const m: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  const seen: i32[] = [];
  for (const x of m[0]) {
    seen.push(x);
    m[0][1] = 9;
  }
  line("fixed-element", seen);
}

function dynamicElementSubject(): void {
  const m: FixedArray<i32, 2>[] = [[1, 2]];
  const seen: i32[] = [];
  for (const x of m[0]) {
    seen.push(x);
    m[0][1] = 9;
  }
  line("dynamic-element", seen);
}

function rebindHandle(): void {
  let r = new Holder();
  const s = new Holder();
  s.a[1] = 7;
  const seen: i32[] = [];
  for (const x of r.a) {
    seen.push(x);
    r = s;
  }
  line("rebind", seen);
}

function rebindIndex(): void {
  const m: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  let i: i32 = 0;
  const seen: i32[] = [];
  for (const x of m[i]) {
    seen.push(x);
    i = 1;
  }
  line("index", seen);
}


function onceOnly(): void {
  const r = new Holder();
  const m: FixedArray<i32, 2>[] = [[5, 6]];
  const calls: i32[] = [0];
  const seen: i32[] = [];
  for (const x of m[countCall(calls)]) {
    seen.push(x);
  }
  for (const x of made(r, calls).a) {
    seen.push(x);
  }
  line("once", seen);
  line("calls", calls);
}

function countCall(calls: i32[]): i32 {
  calls[0] = calls[0] + 1;
  return 0;
}

function made(r: Holder, calls: i32[]): Holder {
  calls[0] = calls[0] + 1;
  return r;
}

function bindingCopy(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  const seen: i32[] = [];
  for (let x of b) {
    x = x + 10;
    seen.push(x);
  }
  line("binding", seen);
  line("after", [b[0], b[1]]);
}

function rows(): void {
  const grid: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  const seen: i32[] = [];
  for (const row of grid) {
    seen.push(row[0]);
    seen.push(row[1]);
    grid[1][1] = 9;
  }
  line("rows", seen);
}

function growth(): void {
  const m: FixedArray<i32, 2>[] = [[1, 2]];
  const seen: i32[] = [];
  for (const x of m[0]) {
    seen.push(x);
    for (let i: i32 = 0; i < 64; i += 1) {
      m.push([0, 0]);
    }
    m[0][1] = 9;
  }
  line("growth", seen);
}

function earlyExit(): void {
  const b: FixedArray<i32, 3> = [1, 2, 3];
  let n: i32 = 0;
  for (const x of b) {
    n += x;
    if (x === 2) {
      break;
    }
    b[1] = 2;
  }
  line("exit", [n]);
}

function callbacks(): void {
  const seen: i32[] = [];
  g.forEach((x: i32): void => {
    seen.push(x);
    g[1] = 9;
  });
  line("forEach", seen);
  g[1] = 2;
  const r = new Holder();
  const mapped = r.a.map((x: i32, i: i32): i32 => {
    r.a[1] = 8;
    return x * 10 + i;
  });
  line("map", mapped);
  r.a[1] = 2;
  const kept = r.a.filter((x: i32): boolean => {
    r.a[1] = 7;
    return x > 1;
  });
  line("filter", kept);
  r.a[1] = 2;
  const some = r.a.some((x: i32): boolean => {
    r.a[1] = 6;
    return x === 6;
  });
  print(`some ${some}`);
  r.a[1] = 2;
  const every = r.a.every((x: i32): boolean => {
    r.a[1] = 0;
    return x > 0;
  });
  print(`every ${every}`);
  r.a[1] = 2;
  const found = r.a.findIndex((x: i32, i: i32): boolean => {
    r.a[1] = 5;
    return x === 5 && i === 1;
  });
  print(`findIndex ${found}`);
  r.a[1] = 2;
  const sum = r.a.reduce((acc: i32, x: i32, i: i32): i32 => {
    r.a[1] = 4;
    return acc * 10 + x + i;
  }, 0);
  print(`reduce ${sum}`);
  r.a[1] = 2;
  const right = r.a.reduceRight((acc: i32, x: i32, i: i32): i32 => {
    r.a[0] = 3;
    return acc * 10 + x + i;
  }, 0);
  print(`reduceRight ${right}`);
}

function namedCallback(x: i32): void {
  print(`named ${x}`);
  g[1] = 6;
}

function valueCallback(): void {
  namedCallback(0);
  g[1] = 2;
  g.forEach(namedCallback);
  const f: (x: i32) => void = namedCallback;
  g[1] = 2;
  g.forEach(f);
  g[1] = 2;
}

export function main(): void {
  localSubject();
  globalSubject();
  fieldSubject();
  nestedFieldSubject();
  fixedElementSubject();
  dynamicElementSubject();
  rebindHandle();
  rebindIndex();
  onceOnly();
  bindingCopy();
  rows();
  growth();
  earlyExit();
  callbacks();
  valueCallback();
}
