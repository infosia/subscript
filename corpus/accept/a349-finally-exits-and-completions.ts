// corpus: accept/a349-finally-exits-and-completions
// purpose: Finalizers preserve and replace completions in lexical cleanup order.
// exercises: finally, return, throw, break, continue, using, counted-return
// questions: C6, Q9, Q34
// tsc: accepts; js-comparable: yes
class Resource {
  tag: string;
  constructor(tag: string) { this.tag = tag; }
  [Symbol.dispose](): void { print(`dispose ${this.tag}`); }
}
function normal(): void {
  try { print("body"); } finally { print("normal finally"); }
  try { throw new Error("caught"); }
  catch (e) { if (e instanceof Error) { print(e.message); } }
  finally { print("catch finally"); }
}
function saved(): i32 {
  let x: i32 = 99;
  try { const x: i32 = 7; return x; }
  finally { print(`${x}`); x = 100; }
}
function replaced(): i32 { try { return 1; } finally { return 2; } }
function suppressed(): i32 { try { throw new Error("old"); } finally { return 3; } }
function fail(): void { throw new Error("new"); }
function exceptional(): void {
  try {
    try { throw new Error("old"); } finally { print("finally"); fail(); }
  } catch (e) { if (e instanceof Error) { print(e.message); } }
  try {
    try { return; } finally { throw new Error("return replaced"); }
  } catch (e) { if (e instanceof Error) { print(e.message); } }
}
function loops(): void {
  for (let i: i32 = 0; i < 2; i++) {
    try { print(`${i}`); continue; } finally { print(`finally ${i}`); }
  }
  while (true) { try { throw new Error("cancelled"); } finally { print("break"); break; } }
  for (let i: i32 = 0; i < 2; i++) {
    try { break; } finally { print(`continue ${i}`); continue; }
  }
  try { print("local loop"); } finally {
    for (let j: i32 = 0; j < 2; j++) { if (j == 0) { continue; } break; }
    print("local loop end");
  }
}
function nested(): void {
  using outer = new Resource("outer");
  try {
    try { using inner = new Resource("inner"); print("nested"); }
    finally { using local = new Resource("finally"); print("inner finally"); }
  } finally { print("outer finally"); }
  try {
    try { throw new Error("first"); } catch { print("catch throws"); throw new Error("second"); }
    finally { print("throwing catch finally"); }
  } catch (e) { if (e instanceof Error) { print(e.message); } }
}
async function value(n: i32): Promise<i32> { return n; }
function counted(): Promise<i32> {
  try { return value(10); } finally { return value(20); }
}
export async function main(): Promise<void> {
  normal(); print(`${saved()}`); print(`${replaced()}`); print(`${suppressed()}`);
  exceptional(); loops(); nested(); const h = counted(); print(`${await h}`);
}

// pin: 03974dd6
// pin-dev-jit: Rejected with S010 at finally; no stdout.
// pin-c-aot: Rejected with S010 at finally; no stdout.
// pin-interpreter: Rejected with S010 at finally; no stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
