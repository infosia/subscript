// corpus: accept/a468-optional-handle-return-exception
// purpose: Optional adoption keeps the exception, exit actions, and queue order of a returned handle.
// observable: The caller catches a rejection after finally; absent paths finish without an adoption turn.
// exercises: async-function, optional-chain, return, try, finally, lambda
// questions: compiler.md §318 rules 2 and 3, §236 rule 2
// tsc: accepts; js-comparable: yes
async function step(): Promise<void> {}
class S {
  async am(fail: boolean): Promise<void> {
    await step();
    print("am");
    if (fail) throw new Error("rejected");
  }
}
class Holder { s: S | null = null; }
function receiver(s: S | null): S | null { print("receiver"); return s; }
async function g(s: S | null, fail: boolean): Promise<void> {
  try { return receiver(s)?.am(fail); }
  catch (e) { print("local catch"); }
  finally { print("finally"); }
}
async function replace(s: S | null): Promise<void> {
  try { return s?.am(false); }
  finally { print("replace"); return undefined; }
}
async function nested(h: Holder | null): Promise<void> { return h?.s?.am(false); }
async function tick(): Promise<void> {
  for (let i: i32 = 0; i < 8; i++) {
    await step();
    print(`turn${i}`);
  }
}
async function timed(s: S | null): Promise<void> {
  const ticker = tick();
  await g(s, false);
  print("done");
  await ticker;
}
export async function main(): Promise<void> {
  try { await g(new S(), true); }
  catch (e) { print("caller catch"); }
  await g(null, true);
  await replace(new S());
  await replace(null);
  const h = new Holder();
  await nested(null);
  await nested(h);
  h.s = new S();
  await nested(h);
  const f: (s: S | null) => Promise<void> = async (s) => s?.am(false);
  await f(new S());
  await f(null);
  print("present");
  await timed(new S());
  print("absent");
  await timed(null);
  print("end");
}
