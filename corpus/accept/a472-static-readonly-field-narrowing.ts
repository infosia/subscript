// corpus: accept/a472-static-readonly-field-narrowing
// purpose: Keeps static readonly narrowing across calls, suspension, stores, and an immediate lambda call.
// exercises: static-readonly, null-narrowing, await, lambda
// questions: compiler.md §322, §304, §162, collisions.md C17
// tsc: accepts; js-comparable: yes

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
function tick(): void {}
class K { static readonly R: A | null = mk(); }
class H { R: A | null = null; }
async function pause(): Promise<void> {}
function row_call(): void {
  if (K.R !== null) { tick(); print(`${K.R.x}`); }
}
async function row_await(): Promise<void> {
  if (K.R !== null) { await pause(); print(`${K.R.x}`); }
}
function row_lambda(): void {
  if (K.R !== null) { print(`${((): i32 => K.R.x)()}`); }
}
function row_store(): void {
  const h = new H();
  if (K.R !== null) { h.R = null; print(`${K.R.x}`); }
}
export async function main(): Promise<void> {
  row_call();
  await row_await();
  row_lambda();
  row_store();
}
