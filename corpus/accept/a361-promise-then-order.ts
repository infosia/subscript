// corpus: accept/a361-promise-then-order
// purpose: then, catch, and finally callbacks run in the node order among await continuations and Promise.all.
// exercises: Promise.then, Promise.catch, Promise.finally, promise-all, async-function, order
// questions: compiler.md §186 rule 3, compiler.md §94, collisions.md C8
// tsc: accepts; js-comparable: yes
async function now(n: i32): Promise<i32> { return n; }
async function later(n: i32): Promise<i32> { await now(0); return n; }
async function fail(n: i32): Promise<i32> { await now(0); throw new Error(`fail ${n}`); }
async function ticker(): Promise<void> {
  for (let i: i32 = 1; i <= 12; i++) { print(`t${i}`); await now(0); }
}
async function awaiter(): Promise<void> {
  print(`awaiter ${await later(10)}`);
  print(`awaiter ${await now(11)}`);
}
export async function main(): Promise<void> {
  const t = ticker();
  const a = awaiter();
  const all = Promise.all([later(1), now(2)]);
  const th = later(3)
    .then((v: i32): i32 => { print(`then ${v}`); return v; })
    .then((v: i32): void => { print(`then2 ${v}`); });
  const adopt = now(4).then((v: i32): Promise<i32> => { print(`adopt ${v}`); return later(v + 1); });
  const caught = fail(6).catch((e: Error): i32 => { print(`catch ${e.message}`); return 6; });
  const fin = now(7).finally((): void => { print("finally 7"); });
  const ah = (async (): Promise<void> => { const xs = await all; print(`all ${xs.join(",")}`); })();
  print("sync");
  await th;
  print("main th");
  print(`main adopt ${await adopt}`);
  print(`main caught ${await caught}`);
  print(`main finally ${await fin}`);
  await ah;
  await a;
  await t;
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 4 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 22:6.
// pin-c-aot: Exit 1; the same 4 errors before C emission.
// pin-interpreter: Checker rejects with the same 4 errors before lowering.
