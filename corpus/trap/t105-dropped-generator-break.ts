// corpus: trap/t105-dropped-generator-break
// purpose: The last generator release reports an unobserved task exception.
// exercises: generator, counted-holder, async-exception
// questions: compiler.md §176 rule 5, §116 rule 4
// tsc: accepts
// js-comparable: no C8: A last release reports an exception synchronously.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception; message Error: lost

let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
async function fail(): Promise<i32> { throw new Error("lost"); }
function* gen(a: Promise<i32>[]): Generator<i32> {
  const local = a;
  consume(local[0]);
  transfer.pop();
  yield 1;
  print("resumed");
  yield 2;
  print("done");
}
async function make(): Promise<Generator<i32>> {
    const h = fail();
    if (false) { await h; }
    return gen([h]);
}
async function use(): Promise<void> {
    for (const v of await make()) { print(`v ${v}`); break; }
    print("after drop");
}
export async function main(): Promise<void> { await use(); print("end"); }

// pin: 879f6778875fbd3277f79d5f4157ac667ff05a6d
// pin-dev-jit: Prints v 1, after drop, end; no trap; 1 retained task.
// pin-c-aot: Prints v 1, after drop, end; no trap; 1 retained task.
// pin-interpreter: Prints v 1, after drop, end; no trap; 1 retained task.
// tsc-version: TypeScript 5.9.2; exit 0.
