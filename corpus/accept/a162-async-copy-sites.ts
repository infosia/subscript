// corpus: accept/a162-async-copy-sites
// purpose: Uses each async-handle copy site through loops, lambdas, indexed storage, conditionals, and discarded pop results.
// exercises: async-handle, for-of, continue, arrow-lambda, index-store, conditional, array-pop
// questions: §70, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
async function copiedWork(value: i32): Promise<i32> {
  await Context.suspend();
  return value;
}

function printNumber(value: i32): void {
  print((value as f64).toString(10));
}

class HeldArrays {
  forOfHandles: Promise<i32>[];
  continuedHandles: Promise<i32>[];
  indexed: Promise<i32>[];
  popped: Promise<i32>[];

  constructor(
    forOfHandles: Promise<i32>[],
    continuedHandles: Promise<i32>[],
    indexed: Promise<i32>[],
    popped: Promise<i32>[],
  ) {
    this.forOfHandles = forOfHandles;
    this.continuedHandles = continuedHandles;
    this.indexed = indexed;
    this.popped = popped;
  }
}

async function exerciseCopySites(): Promise<HeldArrays> {
  const forOfHandles: Promise<i32>[] = [copiedWork(1), copiedWork(2)];
  let forOfTotal: i32 = 0;
  for (const handle of forOfHandles) {
    forOfTotal += await handle;
  }
  printNumber(forOfTotal);
  const forOfAgain: i32 = await forOfHandles[0];
  printNumber(forOfAgain);

  const continuedHandles: Promise<i32>[] = [
    copiedWork(10),
    copiedWork(20),
    copiedWork(30),
  ];
  let continuedTotal: i32 = 0;
  for (let i: i32 = 0; i < continuedHandles.length; i += 1) {
    if (i === 1) {
      continue;
    }
    const handle: Promise<i32> = continuedHandles[i];
    continuedTotal += await handle;
  }
  printNumber(continuedTotal);

  const make: () => Promise<i32> = (): Promise<i32> => copiedWork(4);
  const made: Promise<i32> = make();
  const madeValue: i32 = await made;
  printNumber(madeValue);

  const indexed: Promise<i32>[] = [copiedWork(60)];
  await indexed[0];
  const indexSource: Promise<i32> = copiedWork(6);
  indexed[0] = indexSource;
  const indexValue: i32 = await indexed[0];
  printNumber(indexValue);

  const flag: boolean = true;
  const conditional: Promise<i32> = flag ? copiedWork(7) : copiedWork(70);
  const conditionalValue: i32 = await conditional;
  printNumber(conditionalValue);

  const popped: Promise<i32>[] = [copiedWork(8)];
  const poppedValue: i32 = await popped[0];
  printNumber(poppedValue);
  popped.pop();

  printNumber(256);
  return new HeldArrays(forOfHandles, continuedHandles, indexed, popped);
}

export async function main(): Promise<void> {
  const held: HeldArrays = await exerciseCopySites();
  const forOfHandles: Promise<i32>[] = held.forOfHandles;
  const continuedHandles: Promise<i32>[] = held.continuedHandles;
  const indexed: Promise<i32>[] = held.indexed;
  const popped: Promise<i32>[] = held.popped;
  Context.free(held);
  await Context.suspend();
  Context.collect();
  if (
    forOfHandles.length +
      continuedHandles.length +
      indexed.length +
      popped.length !==
    6
  ) {
    unreachable();
  }
}
