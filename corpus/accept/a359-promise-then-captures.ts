// corpus: accept/a359-promise-then-captures
// purpose: A synchronous callback of then, catch, or finally owns its const captures while the chain is pending.
// exercises: Promise.then, Promise.catch, Promise.finally, capture, collect, counted-capture, adoption
// questions: compiler.md §186 rule 4, compiler.md §181, collisions.md C8
// tsc: accepts; js-comparable: yes
class Box { n: i32 = 0; constructor(n: i32) { this.n = n; } }
async function later(n: i32): Promise<i32> { await Context.suspend(); return n; }
async function fail(n: i32): Promise<i32> { await Context.suspend(); throw new Error(`fail ${n}`); }
function* count(from: i32): Generator<i32> { yield from; yield from + 1; }
function start(k: i32): Promise<string> {
  const base: i32 = k * 10;
  const label: string = `L${k}`;
  const box = new Box(k);
  return later(k).then((v: i32): string => `${label} ${v + base} ${box.n}`);
}
function counted(): Promise<Promise<i32>[]> {
  const g = count(7);
  const hs: Promise<i32>[] = [later(1), later(2)];
  return later(0).then((v: i32): Promise<i32>[] => {
    let text = "";
    for (const x of g) { text = `${text}${x} `; }
    print(`${text}${hs.length + v}`);
    return hs;
  });
}
function adopt(n: i32): Promise<i32> {
  const h = later(n);
  return later(0).then((v: i32): Promise<i32> => h);
}
function churn(): void {
  Context.collect();
  const junk: Box[] = [];
  for (let i: i32 = 0; i < 100; i++) { junk.push(new Box(-1)); }
}
export async function main(): Promise<void> {
  const hs: Promise<string>[] = [];
  for (let i: i32 = 1; i <= 3; i++) { hs.push(start(i)); }
  const c = counted();
  const d = adopt(5);
  churn();
  for (const h of hs) { print(await h); }
  for (const h of await c) { print(`counted ${await h}`); }
  print(`adopted ${await d}`);
  const k: i32 = 42;
  const label: string = "done";
  const f = later(1).finally((): void => { print(`finally ${label}`); });
  const r = fail(2).catch((e: Error): i32 => k + e.message.length);
  churn();
  print(`${await f}`);
  print(`${await r}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 8 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 14:19.
// pin-c-aot: Exit 1; the same 8 errors before C emission.
// pin-interpreter: Checker rejects with the same 8 errors before lowering.
