// corpus: accept/a346-dropped-generator-frame
// purpose: A dropped generator releases its frame without body effects.
// exercises: generator, counted-holder, field-free, array-pop, alias
// questions: compiler.md §176
// tsc: accepts
// js-comparable: no Q6: Context.free has no JavaScript shim.

let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
async function work(): Promise<i32> { return 7; }
function* gen(a: Promise<i32>[]): Generator<i32> {
  const local = a;
  consume(local[0]);
  transfer.pop();
  yield 1;
  print("resumed");
  yield 2;
  print("done");
}
class Holder { g: Generator<i32> = gen([]); }
async function use(shape: i32): Promise<void> {
  const h = work();
  await h;
  if (shape == 0) {
    for (const v of gen([h])) { print(`v ${v}`); break; }
  } else if (shape == 1) {
    for (const v of gen([h])) { print(`v ${v}`); return; }
  } else if (shape == 2) {
    const it = gen([h]);
    print(`v ${it.next().value}`);
  } else if (shape == 3) {
    const it = gen([h]);
    print("unused");
  } else if (shape == 4) {
    const o = new Holder();
    o.g = gen([h]);
    print(`v ${o.g.next().value}`);
    Context.free(o);
  } else if (shape == 5) {
    const gs: Generator<i32>[] = [gen([h])];
    print(`v ${gs[0].next().value}`);
    gs.pop();
  } else {
    const it = gen([h]);
    { const copy = it; print(`v ${copy.next().value}`); }
    print(`v ${it.next().value}`);
    print(`exhaust ${it.next().done}`);
  }
}
export async function main(): Promise<void> {
  for (let shape: i32 = 0; shape < 7; shape++) {
    print(`shape ${shape}`);
    await use(shape);
    print("end");
  }
}

// pin: 879f6778875fbd3277f79d5f4157ac667ff05a6d
// pin-dev-jit: Output matches the golden; 6 retained tasks.
// pin-c-aot: Output matches the golden; 6 retained tasks.
// pin-interpreter: Output matches the golden; 6 retained tasks.
// tsc-version: TypeScript 5.9.2; exit 0.
// node-version: v24.18.0; exit 0; output matches the golden with Context adapters.
