// corpus: accept/a351-generator-close-runs-finally
// purpose: Consumer exits close suspended generators before the holder release.
// exercises: generator, finally, break, return, throw, alias, yield
// questions: C6, C8, Q9
// tsc: accepts
// js-comparable: yes
function* values(tag: string): Generator<i32> {
  try {
    try { yield 1; print(`body ${tag}`); yield 2; }
    finally { print(`inner ${tag}`); }
  } finally { print(`outer ${tag}`); }
}
function consume(): void {
  for (const value of values("return")) { print(`return ${value}`); return; }
}
function* suspended(): Generator<i32> {
  try { yield 3; print("unreachable body"); }
  finally { print("close starts"); yield 4; print("close resumes"); }
}
export function main(): void {
  const iterator = values("break");
  const alias = iterator;
  for (const value of iterator) { print(`break ${value}`); break; }
  print(`closed ${alias.next().done}`);
  consume();
  try {
    for (const value of values("throw")) { print(`throw ${value}`); throw new Error("consumer"); }
  } catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  for (const value of values("exhaust")) { print(`exhaust ${value}`); }
  { const dropped = values("drop"); print(`drop ${dropped.next().value}`); }
  const finalizer = suspended();
  for (const value of finalizer) { print(`suspended ${value}`); break; }
  print(`resumed ${finalizer.next().done}`);
}

// pin: 03974dd6
// pin-dev-jit: Rejected S010 at 11:13; no stdout.
// pin-c-aot: Rejected S010 at 11:13; no stdout.
// pin-interpreter: Rejected S010 at 11:13; no stdout.
