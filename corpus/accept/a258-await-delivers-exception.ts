// corpus: accept/a258-await-delivers-exception
// purpose: An exception that leaves an async body completes its handle, and each await of the handle raises the same object in the awaiting frame.
// exercises: throw, try-catch, async-function, await, held-handle, async-method, rethrow, reference-identity, instanceof-narrowing
// questions: Q9, Q34
// tsc: accepts; js-comparable: yes
let firstSeen: Error | null = null;

async function value(n: i32): Promise<i32> {
  print(`value:${n}`);
  return n;
}

async function failsAtCall(tag: string): Promise<i32> {
  print(`failsAtCall:start ${tag}`);
  throw new Error(`at call ${tag}`);
}

async function failsAfterAwait(tag: string): Promise<i32> {
  print(`failsAfterAwait:start ${tag}`);
  const n: i32 = await value(10);
  print(`failsAfterAwait:resumed ${n}`);
  throw new TypeError(`after await ${tag}`);
}

async function level3(): Promise<i32> {
  print("level3:start");
  await value(3);
  throw new SyntaxError("deep");
}

async function level2(): Promise<i32> {
  print("level2:start");
  try {
    const n: i32 = await level3();
    print(`level2:unreached ${n}`);
    return n;
  } catch (e) {
    if (e instanceof SyntaxError) {
      print(`level2:caught ${e.message}, rethrow`);
    }
    throw e;
  }
}

async function level1(): Promise<i32> {
  print("level1:start");
  const n: i32 = await level2();
  print(`level1:unreached ${n}`);
  return n;
}

class Loader {
  name: string = "loader";

  async load(fail: boolean): Promise<i32> {
    print(`${this.name}:load ${fail}`);
    await value(5);
    if (fail) {
      throw new Error(`${this.name} failed`);
    }
    return 50;
  }
}

async function directForms(): Promise<void> {
  try {
    await failsAtCall("direct");
    print("direct:unreached");
  } catch (e) {
    if (e instanceof Error) {
      print(`direct:caught ${e.message}`);
    }
  }
  try {
    const n: i32 = await failsAfterAwait("direct");
    print(`direct:unreached ${n}`);
  } catch (e) {
    if (e instanceof TypeError) {
      print(`direct:caught ${e.name} ${e.message}`);
    }
  }
  const loader: Loader = new Loader();
  try {
    print(`method:ok ${await loader.load(false)}`);
    print(`method:unreached ${await loader.load(true)}`);
  } catch (e) {
    if (e instanceof Error) {
      print(`method:caught ${e.message}`);
    }
  }
}

async function heldForms(): Promise<void> {
  const early: Promise<i32> = failsAtCall("held");
  print("held:after call");
  try {
    const n: i32 = await early;
    print(`held:unreached ${n}`);
  } catch (e) {
    if (e instanceof Error) {
      firstSeen = e;
      print(`held:first ${e.message}`);
    }
  }
  try {
    const n: i32 = await early;
    print(`held:unreached ${n}`);
  } catch (e) {
    const first: Error | null = firstSeen;
    if (e instanceof Error && first !== null) {
      print(`held:second ${e.message}`);
      print(`held:same object ${e === first}`);
    }
  }
  const late: Promise<i32> = failsAfterAwait("held");
  print("held:late created");
  try {
    await late;
  } catch (e) {
    if (e instanceof TypeError) {
      print(`held:late caught ${e.message}`);
    }
  }
}

async function chain(): Promise<void> {
  try {
    const n: i32 = await level1();
    print(`chain:unreached ${n}`);
  } catch (e) {
    if (e instanceof SyntaxError) {
      print(`chain:caught ${e.name} ${e.message}`);
    }
  }
}

async function spanning(): Promise<i32> {
  let progress: i32 = 0;
  try {
    const a: i32 = await value(1);
    progress += a;
    print(`spanning:a progress=${progress}`);
    const b: i32 = await value(2);
    progress += b;
    print(`spanning:b progress=${progress}`);
    const c: i32 = await failsAfterAwait("spanning");
    progress += c;
    print(`spanning:unreached progress=${progress}`);
  } catch (e) {
    if (e instanceof TypeError) {
      print(`spanning:caught ${e.message} progress=${progress}`);
    }
    progress += 100;
  }
  const d: i32 = await value(4);
  progress += d;
  print(`spanning:after handler progress=${progress}`);
  const e: i32 = await value(6);
  return progress + e;
}

export async function main(): Promise<void> {
  await directForms();
  await heldForms();
  await chain();
  const total: i32 = await spanning();
  print(`main:total=${total}`);
}
