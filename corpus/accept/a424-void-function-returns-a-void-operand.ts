// corpus: accept/a424-void-function-returns-a-void-operand
// purpose: A void function returns a void operand: `return e` with a `void` operand is `e; return;`, and an async `Promise<void>` return of a handle adopts it.
// observable: each form logs its calls in order; a ticker task logs each turn, and each adopting form logs its result in the turn where node completes it.
// exercises: void, async-function, async-return, lambda, forEach, exception, await
// questions: compiler.md §237, §236, collisions.md C21
// tsc: accepts; js-comparable: yes
const log: string[] = [];
const ys: i32[] = [];

function v(): void {
  log.push("v");
}

function g(x: i32): void {
  log.push(`g${x}`);
}

async function step(): Promise<i32> {
  return 0;
}

async function ticker(tag: string): Promise<void> {
  for (let i: i32 = 1; i <= 6; i++) {
    log.push(`${tag}turn${i}`);
    await step();
  }
}

async function settledWork(): Promise<void> {
  log.push("sw");
}

async function pendingWork(): Promise<void> {
  await step();
  log.push("pw-mid");
  await step();
  log.push("pw-done");
}

async function failingWork(): Promise<void> {
  await step();
  log.push("fw-mid");
  throw new Error("work-failed");
}

function returnsVoid(): void {
  return v();
}

const arrowVoid = (): void => v();

async function awaitWork(): Promise<void> {
  return await settledWork();
}

async function adoptSettled(): Promise<void> {
  return settledWork();
}

async function adoptPending(): Promise<void> {
  return pendingWork();
}

async function adoptFailing(): Promise<void> {
  return failingWork();
}

async function mixed(b: boolean): Promise<void> {
  return b ? settledWork() : v();
}

function chooseVoid(b: boolean): void {
  return b ? v() : g(9);
}

function inTryFinally(): void {
  try {
    log.push("try");
    return v();
  } finally {
    log.push("finally");
  }
}

async function runOne(tag: string, f: () => Promise<void>): Promise<void> {
  const t: Promise<void> = ticker(tag);
  try {
    await f();
    log.push(`${tag}-done`);
  } catch (e) {
    if (e instanceof Error) {
      log.push(`${tag}!${e.message}`);
    }
  }
  await t;
}

export async function main(): Promise<void> {
  const xs: i32[] = [1, 2];
  returnsVoid();
  arrowVoid();
  inTryFinally();
  chooseVoid(true);
  chooseVoid(false);
  xs.forEach((x: i32) => g(x));
  xs.forEach((x: i32) => ys.push(x));
  const cb: (x: i32) => void = (x: i32) => ys.push(x * 10);
  cb(3);
  const block: (x: i32) => void = (x: i32) => {
    log.push(`block${x}`);
    return ys.push(x * 100);
  };
  block(4);
  log.push(`ys=${ys.join(",")}`);
  print(log.join(" "));
  log.length = 0;
  await runOne("aw", awaitWork);
  await runOne("as", adoptSettled);
  await runOne("ap", adoptPending);
  await runOne("af", adoptFailing);
  await runOne("mh", (): Promise<void> => mixed(true));
  await runOne("mv", (): Promise<void> => mixed(false));
  print(log.join(" "));
}
