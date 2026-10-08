// corpus: accept/a352-finalizer-walker-controls/main
// purpose: Keeps finalizer effects and runs loop updates after finalizers.
// exercises: finally, value-type-method, synthetic-prefix, continue, generator-return, using
// questions: C6, Q9, Q34
// tsc: accepts; js-comparable: yes
import { ValueType } from "./lib";
@ValueType
class Counter {
  n: i32 = 0;
  bump(): void { this.n++; print(`counter ${this.n}`); }
}
class FailingResource {
  [Symbol.dispose](): void { throw new Error("hook"); }
}
function onlyFinalizer(): i32 {
  try { return 1; } finally { throw new Error("only finalizer"); }
}
class Limit { n: i32 = 2; }
function limit(): Limit | null { return new Limit(); }
function* returns(): Generator<i32> {
  try { yield 4; return; } finally { print("h fin"); }
}
export function main(): void {
  try { onlyFinalizer(); }
  catch (e) { if (e instanceof Error) { print(e.message); } }
  const c = new Counter();
  try { print("value body"); } finally { c.bump(); }
  for (let i: i32 = 0; i < (limit()?.n ?? 0); i++) {
    try { print(`body ${i}`); continue; }
    finally { print(`body fin ${i}`); }
  }
  for (let i: i32 = 0; i < (limit()?.n ?? 0); i++) {
    try { print(`finalizer body ${i}`); break; }
    finally { print(`continue fin ${i}`); continue; }
  }
  for (const n of returns()) { print(`${n}`); }
  try {
    try { throw new Error("held local"); }
    finally {
      try { using r = new FailingResource(); }
      catch (e) { if (e instanceof Error) { print(`local ${e.message}`); } }
    }
  } catch (e) { if (e instanceof Error) { print(`outer ${e.message}`); } }
  try {
    try { throw new Error("held replaced"); }
    finally { using r = new FailingResource(); }
  } catch (e) { if (e instanceof Error) { print(`replaced ${e.message}`); } }
}
// pin: 03974dd6
// pin-dev-jit: Rejected with S010 at finally; no stdout.
// pin-c-aot: Rejected with S010 at finally; no stdout.
// pin-interpreter: Rejected with S010 at finally; no stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
