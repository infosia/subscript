// corpus: accept/a161-counted-handle-stores
// purpose: Keeps async handles valid across global, field, index, and spread stores after source-scope release.
// exercises: async-handle, global-store, field-store, index-store, array-spread, frame-reuse
// questions: §70, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
async function storedWork(value: i32): Promise<i32> {
  await Context.suspend();
  return value;
}

let globalHandle: Promise<i32> = storedWork(90);

class HandleHolder {
  handle: Promise<i32>;

  constructor(handle: Promise<i32>) {
    this.handle = handle;
  }
}

async function reuseFrames(): Promise<void> {
  await storedWork(100);
  await storedWork(101);
  await storedWork(102);
  await storedWork(103);
}

function printNumber(value: i32): void {
  print((value as f64).toString(10));
}

async function exerciseStores(): Promise<void> {
  {
    const source: Promise<i32> = storedWork(3);
    globalHandle = source;
    await source;
  }
  await reuseFrames();
  const globalValue: i32 = await globalHandle;
  printNumber(globalValue);

  {
    const holder: HandleHolder = new HandleHolder(storedWork(90));
    {
      const source: Promise<i32> = storedWork(5);
      holder.handle = source;
      await source;
    }
    await reuseFrames();
    const fieldValue: i32 = await holder.handle;
    printNumber(fieldValue);
    Context.free(holder);
  }

  {
    const indexed: Promise<i32>[] = [storedWork(90)];
    {
      const source: Promise<i32> = storedWork(7);
      indexed[0] = source;
      await source;
    }
    await reuseFrames();
    const indexValue: i32 = await indexed[0];
    printNumber(indexValue);
  }

  {
    let spread: Promise<i32>[] = [];
    {
      const source: Promise<i32>[] = [storedWork(1), storedWork(2)];
      spread = [...source];
      await source[0];
      await source[1];
    }
    await reuseFrames();
    const spread0: i32 = await spread[0];
    const spread1: i32 = await spread[1];
    printNumber(spread0);
    printNumber(spread1);
  }

  globalHandle = globalHandle;
  printNumber(24);
}

export async function main(): Promise<void> {
  await exerciseStores();
  await Context.suspend();
  Context.collect();
}
