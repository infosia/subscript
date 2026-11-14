// corpus: accept/a455-a-default-reads-a-captured-local-and-an-async-default-fails-the-handle
// purpose: Defaults read captured locals and async default throws fail the handle.
// observable: Captures come from the callee; default effects precede failed handles.
// exercises: lambda, capture, default-argument, self-call, async-arrow, exception
// questions: compiler.md §291, §288, §57
// tsc: accepts; js-comparable: yes
const log: string[] = [];
class Box { value: i32 = 9; }
function first(): i32 { log.push("first"); return 1; }
function fail(): i32 { log.push("fail"); throw new Error("failed"); }
async function af(a: i32 = first(), b: i32 = fail()): Promise<void> { log.push("bad function body"); }
export async function main(): Promise<void> {
  const k: i32 = 5;
  const cap = (x: i32 = k + 1): i32 => x;
  log.push(`${cap()}`);
  log.push(`${cap(8)}`);
  const text: string = "captured";
  const str = (s: string = text): string => s;
  log.push(str());
  const box = new Box();
  const ref = (b: Box = box): i32 => b.value;
  log.push(`${ref()}`);
  const earlier = (a: i32, b: i32 = a + k): i32 => b;
  log.push(`${earlier(2)}`);
  const nested = (): i32 => { const k: i32 = 100; return cap(); };
  log.push(`${nested()}`);
  const rec = (n: i32, x: i32 = k): void => {
    log.push(`${n}:${x}`);
    if (n > 0) rec(n - 1);
  };
  rec(1);
  try { const p = af(); log.push("after function call"); await p; }
  catch (e) { if (e instanceof Error) log.push(`caught ${e.message}`); }
  const al = async (a: i32 = first(), b: i32 = fail()): Promise<void> => { log.push("bad lambda body"); };
  try { const p = al(); log.push("after lambda call"); await p; }
  catch (e) { if (e instanceof Error) log.push(`caught ${e.message}`); }
  print(log.join(" "));
}
