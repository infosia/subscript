// corpus: accept/a358-promise-catch-finally
// purpose: catch recovers a rejection, then takes a rejection callback, and finally runs on each completion.
// exercises: Promise.then, Promise.catch, Promise.finally, exception, async-function
// questions: compiler.md §186, compiler.md §180, collisions.md C8
// tsc: accepts; js-comparable: yes
async function value(n: i32): Promise<i32> { await Context.suspend(); return n; }
async function fail(n: i32): Promise<i32> { await Context.suspend(); throw new Error(`fail ${n}`); }
async function failVoid(): Promise<void> { await Context.suspend(); throw new Error("void failure"); }
function boom(v: i32): i32 { throw new Error(`callback ${v}`); }
export async function main(): Promise<void> {
  try {
    await value(1).then(boom).then((v: i32): i32 => { print("not reached"); return v; });
  } catch (e) {
    if (e instanceof Error) { print(`caught ${e.message}`); }
  }
  print(`recovered ${await fail(1).catch((e: Error): i32 => e.message.length)}`);
  print(`passed ${await value(5).catch(e => 0)}`);
  print(`through ${await fail(2).then((v: i32): i32 => v + 1).catch(e => -1)}`);
  print(await fail(3).then((v: i32): string => `ok ${v}`, (e: Error): string => `rejected ${e.message}`));
  print(await value(3).then((v: i32): string => `ok ${v}`, (e: Error): string => `rejected ${e.message}`));
  await failVoid().catch((e: Error): void => { print(`void ${e.message}`); });
  print(`finally value ${await value(4).finally((): void => { print("finally a"); })}`);
  try {
    await fail(5).finally((): void => { print("finally b"); });
  } catch (e) {
    if (e instanceof Error) { print(`rethrown ${e.message}`); }
  }
  try {
    await value(6).finally((): void => { throw new Error("from finally"); });
  } catch (e) {
    if (e instanceof Error) { print(`replaced ${e.message}`); }
  }
  try {
    await fail(7).catch((e: Error): i32 => { throw new Error(`again ${e.message}`); });
  } catch (e) {
    if (e instanceof Error) { print(`caught ${e.message}`); }
  }
  print(`chained ${await fail(8).catch((e: Error): i32 => 8).finally((): void => { print("finally c"); })}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 21 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 12:20.
// pin-c-aot: Exit 1; the same 21 errors before C emission.
// pin-interpreter: Checker rejects with the same 21 errors before lowering.
