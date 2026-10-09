// corpus: accept/a357-promise-then-chains
// purpose: then on a handle chains callbacks, adopts a returned handle, and stores its handle.
// exercises: Promise.then, async-function, held-handle, adoption, function-value, promise-all, counted-result
// questions: compiler.md §186, collisions.md C8
// tsc: accepts; js-comparable: yes
async function value(n: i32): Promise<i32> { await Context.suspend(); return n; }
async function nothing(): Promise<void> { await Context.suspend(); }
function double(v: i32): i32 { return v * 2; }
function* count(from: i32): Generator<i32> { yield from; yield from + 1; }
class Holder { h: Promise<i32>; constructor(h: Promise<i32>) { this.h = h; } }
function produce(n: i32): Promise<string> { return value(n).then((v: i32): string => `produced ${v}`); }
export async function main(): Promise<void> {
  print(await value(1).then((v: i32): i32 => v + 1).then((v: i32): string => `s${v}`));
  const h = value(10);
  const h2 = h.then(v => v * 2);
  print(`held ${await h2}`);
  print(`adopt ${await value(1).then((v: i32): Promise<i32> => value(v + 100))}`);
  print(`adopt chain ${await value(2).then(v => value(v * 3)).then(v => v + 1)}`);
  await value(3).then((v: i32): void => { print(`void ${v}`); });
  print(`no value ${await nothing().then((): i32 => 9)}`);
  print(`ignored ${await value(4).then((): string => "four")}`);
  const g = await value(3).then((v: i32): Generator<i32> => count(v));
  for (const x of g) { print(`generator ${x}`); }
  const hs = await value(2).then((v: i32): Promise<i32>[] => [value(v), value(v + 1)]);
  for (const x of hs) { print(`handle ${await x}`); }
  const o = new Holder(value(21).then(double));
  print(`field ${await o.h}`);
  const all = await Promise.all([value(3).then(double), value(4).then(double)]);
  print(`all ${all.join(",")}`);
  const h1 = value(1);
  const a = h1.then((v: i32): i32 => v + 10);
  const b = h1.then((v: i32): i32 => v + 20);
  print(`fan-out ${await h1} ${await a} ${await b}`);
  const p = produce(5);
  print(await p);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 22 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 11:61.
// pin-c-aot: Exit 1; the same 22 errors before C emission.
// pin-interpreter: Checker rejects with the same 22 errors before lowering.
