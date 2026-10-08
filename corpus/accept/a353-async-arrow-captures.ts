// corpus: accept/a353-async-arrow-captures
// purpose: Async arrows own immutable captures and root their environments across suspension.
// exercises: async-arrow, capture, handle, reference, string, field, array, collect
// questions: compiler.md §181
// tsc: accepts; js-comparable: yes
async function value(n: i32): Promise<i32> { return n; }
async function later(f: (n: i32) => Promise<i32>): Promise<i32> { await value(0); return await f(3); }
async function zero(): Promise<i32> { return 0; }
class Box { n: i32 = 7; f: () => Promise<i32> = zero; }
function make(): Box { const base: i32 = 7; const b = new Box(); b.f = async (): Promise<i32> => { await value(0); return base; }; return b; }
function capturedHandle(): () => Promise<i32> { const h = value(7); return async (): Promise<i32> => { await Context.suspend(); Context.collect(); return await h; }; }
function startCapturedHandle(): Promise<i32> { return capturedHandle()(); }
export async function main(): Promise<void> {
  const base: i32 = 7;
  print(`direct ${await (async (n: i32): Promise<i32> => { await value(0); return n + base; })(3)}`);
  const f = async (n: i32): Promise<i32> => { await value(0); return n + base; };
  const h = f(3); print(`held ${await h}`);
  print(`later ${await later(f)}`);
  const box = make(); print(`field ${await box.f()}`);
  const fs: (() => Promise<i32>)[] = [];
  for (let i: i32 = 0; i < 3; i++) { const n: i32 = i; fs.push(async (): Promise<i32> => { await value(0); return n; }); }
  for (const job of fs) { print(`loop ${await job()}`); }
  const escaped = startCapturedHandle(); print(`handle ${await escaped}`);
  const b = new Box(); const ref = async (): Promise<i32> => { await value(0); return b.n; }; b.n = 8;
  print(`reference ${await ref()}`);
  const s: string = "seven"; const text = async (): Promise<string> => { await value(0); return s; }; print(await text());
}
