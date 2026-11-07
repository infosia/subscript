// corpus: accept/a423-async-return-adopts-a-handle
// purpose: An async return of a handle adopts it: one turn of the queue, then an await of the handle, in the order of turns of node.
// observable: a ticker task logs each turn, and each form logs its result in the turn where node completes it.
// exercises: async-function, async-arrow, async-return, held-handle, Promise.then, exception, await
// questions: compiler.md §236, §167, §233
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";

const log: string[] = [];

async function step(): Promise<i32> {
  return 0;
}

async function ticker(tag: string): Promise<void> {
  for (let i: i32 = 1; i <= 6; i++) {
    log.push(`${tag}turn${i}`);
    await step();
  }
}

async function settled(): Promise<i32> {
  return 7;
}

async function pending(): Promise<i32> {
  await step();
  await step();
  return 8;
}

async function thrower(): Promise<i32> {
  throw new Error("boom");
}

async function adoptSettled(): Promise<i32> {
  return settled();
}

async function adoptPending(): Promise<i32> {
  return pending();
}

async function awaitSettled(): Promise<i32> {
  return await settled();
}

async function awaitPending(): Promise<i32> {
  return await pending();
}

async function heldSettled(): Promise<i32> {
  const h: Promise<i32> = settled();
  await h;
  return await h;
}

async function heldPending(): Promise<i32> {
  const h: Promise<i32> = pending();
  await h;
  return await h;
}

async function adoptThen(): Promise<Mode> {
  return settled().then((x: i32) => "a");
}

async function adoptThrow(): Promise<i32> {
  return thrower();
}

async function value(): Promise<i32> {
  return 3;
}

async function logged(): Promise<i32> {
  await step();
  log.push("p-mid");
  await step();
  log.push("p-done");
  return 8;
}

async function failing(): Promise<i32> {
  await step();
  log.push("f-mid");
  throw new Error("late");
}

async function inTryCatch(): Promise<i32> {
  try {
    return failing();
  } catch (e) {
    log.push("catch");
    return -1;
  } finally {
    log.push("finally");
  }
}

async function inTryFinally(): Promise<i32> {
  try {
    return logged();
  } finally {
    log.push("finally");
  }
}

class Res {
  name: string;
  constructor(name: string) {
    this.name = name;
  }

  [Symbol.dispose](): void {
    log.push(`dispose ${this.name}`);
  }
}

async function withUsing(): Promise<i32> {
  using r = new Res("r");
  return logged();
}

function* gen(): Generator<i32> {
  try {
    yield 1;
  } finally {
    log.push("gen-finally");
  }
}

async function inLoop(): Promise<i32> {
  for (const x of gen()) {
    return logged();
  }
  return 0;
}

async function choose(b: boolean): Promise<i32> {
  return b ? settled() : logged();
}

async function chooseHeld(b: boolean, h1: Promise<i32>, h2: Promise<i32>): Promise<i32> {
  return b ? h1 : h2;
}

async function finallyThrows(): Promise<i32> {
  try {
    return settled();
  } finally {
    log.push("ft");
    throw new Error("from-finally");
  }
}

async function finallyValue(): Promise<i32> {
  try {
    return settled();
  } finally {
    log.push("fv");
    return 9;
  }
}

async function finallyHandle(): Promise<i32> {
  try {
    return settled();
  } finally {
    log.push("fh");
    return logged();
  }
}

class Bad {
  [Symbol.dispose](): void {
    log.push("d-bad");
    throw new Error("dispose");
  }
}

async function disposeThrows(): Promise<i32> {
  using b = new Bad();
  return settled();
}

async function runCaught(tag: string, f: () => Promise<i32>): Promise<void> {
  const t: Promise<void> = ticker(tag);
  try {
    const r: i32 = await f();
    log.push(`${tag}=${r}`);
  } catch (e) {
    if (e instanceof Error) {
      log.push(`${tag}!${e.message}`);
    }
  }
  await t;
}

async function mixed(b: boolean): Promise<i32> {
  return b ? settled() : 5;
}

async function runOne(tag: string, f: () => Promise<i32>): Promise<void> {
  const t: Promise<void> = ticker(tag);
  const r: i32 = await f();
  log.push(`${tag}=${r}`);
  await t;
}

export async function main(): Promise<void> {
  await runOne("as", adoptSettled);
  await runOne("ap", adoptPending);
  await runOne("ws", awaitSettled);
  await runOne("wp", awaitPending);
  await runOne("hs", heldSettled);
  await runOne("hp", heldPending);
  await runOne("v", value);
  await runOne("xh", (): Promise<i32> => mixed(true));
  await runOne("xv", (): Promise<i32> => mixed(false));
  print(log.join(" "));
  log.length = 0;
  const t: Promise<void> = ticker("m");
  const m: Mode = await adoptThen();
  log.push(`m=${m}`);
  await t;
  const arrow = async (p: Promise<i32>): Promise<i32> => p;
  const a: i32 = await arrow(settled());
  log.push(`arrow=${a}`);
  try {
    await adoptThrow();
  } catch (e) {
    if (e instanceof Error) {
      log.push(`caught=${e.message}`);
    }
  }
  print(log.join(" "));
  log.length = 0;
  try {
    await inTryCatch();
  } catch (e) {
    if (e instanceof Error) {
      log.push(`caller=${e.message}`);
    }
  }
  log.push(`tf=${await inTryFinally()}`);
  log.push(`u=${await withUsing()}`);
  log.push(`l=${await inLoop()}`);
  log.push(`c=${await choose(true)}:${await choose(false)}`);
  const first: Promise<i32> = settled();
  const second: Promise<i32> = settled();
  log.push(`ch=${await chooseHeld(false, first, second)}`);
  print(log.join(" "));
  log.length = 0;
  await runCaught("ft", finallyThrows);
  await runCaught("fv", finallyValue);
  await runCaught("fh", finallyHandle);
  await runCaught("db", disposeThrows);
  print(log.join(" "));
}
