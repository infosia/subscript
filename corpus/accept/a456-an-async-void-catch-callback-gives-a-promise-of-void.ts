// corpus: accept/a456-an-async-void-catch-callback-gives-a-promise-of-void
// purpose: An async void catch callback adopts its handle and completes in the node order.
// exercises: Promise.catch, Promise.then, async-function, async-lambda, exception, order
// questions: compiler.md §292, §289, §186
// tsc: accepts; js-comparable: yes
async function wi(): Promise<i32> { return 1; }
async function bad(): Promise<i32> { throw new Error("bad"); }
async function note(log: string[], m: string): Promise<void> { log.push(m); }
async function fail(e: Error): Promise<void> { throw new Error(`callback ${e.message}`); }
export async function main(): Promise<void> {
  const log: string[] = [];
  await bad().catch(async (e: Error) => { log.push(e.message); });
  await bad().catch(async (e: Error) => { await note(log, "n"); });
  await bad().catch((e: Error) => note(log, e.message));
  try {
    await bad().catch(fail);
  } catch (e) {
    if (e instanceof Error) { log.push(e.message); }
  }
  const other = wi()
    .then((n: i32): i32 => { log.push("other 1"); return n; })
    .then((n: i32): i32 => { log.push("other 2"); return n; })
    .then((n: i32): i32 => { log.push("other 3"); return n; })
    .then((): void => { log.push("other 4"); });
  log.push("sync");
  await bad().catch(async (e: Error) => {
    log.push(`order ${e.message}`);
    await note(log, "callback start");
    log.push("callback end");
  });
  log.push("caught done");
  await other;
  log.push("other done");
  const second = wi()
    .then((n: i32): i32 => { log.push("second 1"); return n; })
    .then((): void => { log.push("second 2"); });
  log.push("success sync");
  await wi().catch(async () => { log.push("unexpected callback"); });
  log.push("passed done");
  await second;
  log.push("second done");
  print(log.join("\n"));
}
