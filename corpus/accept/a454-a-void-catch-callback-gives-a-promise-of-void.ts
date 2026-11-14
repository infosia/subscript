// corpus: accept/a454-a-void-catch-callback-gives-a-promise-of-void
// purpose: A void catch callback drops a fulfilled value and handles failure in the node order.
// exercises: Promise.catch, Promise.then, async-function, exception, string, class, order
// questions: compiler.md §289, §186
// tsc: accepts; js-comparable: yes
async function wi(): Promise<i32> { return 1; }
async function bad(): Promise<i32> { throw new Error("bad"); }
function v(): void { }
async function text(): Promise<string> { return `text ${await wi()}`; }
class Box { value: string = "owned"; }
async function box(): Promise<Box> { return new Box(); }
export async function main(): Promise<void> {
  const log: string[] = [];
  await wi().catch(() => v());
  log.push("success");
  await bad().catch((e: Error) => { log.push(e.message); });
  try {
    await bad().catch((e: Error): void => { throw new Error(`callback ${e.message}`); });
  } catch (e) {
    if (e instanceof Error) { log.push(e.message); }
  }
  await text().catch(() => { log.push("unexpected text failure"); });
  log.push("text done");
  await box().catch(v);
  log.push("box done");
  const other = wi()
    .then((n: i32): i32 => { log.push("other 1"); return n; })
    .then((): void => { log.push("other 2"); });
  log.push("sync");
  await bad().catch((e: Error) => { log.push(`order ${e.message}`); });
  log.push("caught done");
  await other;
  log.push("other done");
  const second = wi()
    .then((n: i32): i32 => { log.push("second 1"); return n; })
    .then((): void => { log.push("second 2"); });
  log.push("success sync");
  await wi().catch(v);
  log.push("passed done");
  await second;
  log.push("second done");
  print(log.join("\n"));
}
