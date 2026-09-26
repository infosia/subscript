// corpus: accept/a261-lambda-borrows-a-captured-handle
// purpose: A lambda borrows its captured handle and owns each local copy on normal and exceptional exits.
// exercises: lambda, capture, held-handle, async-function, await, throw, try-catch
// questions: Q9, Q10, Q34
// tsc: accepts; js-comparable: yes
async function value(): Promise<i32> { return 7; }
async function plain(): Promise<void> {
  const h: Promise<i32> = value();
  const f = (): i32 => {
    const local: Promise<i32> = h;
    return 1;
  };
  print(`${f()}`);
  print(`${f()}`);
  print(`got ${await h}`);
}

async function fails(): Promise<i32> { throw new Error("dropped"); }
async function exceptional(): Promise<void> {
  const h: Promise<i32> = fails();
  const f = (skip: boolean): i32 => {
    const local: Promise<i32> = h;
    if (skip) { throw new TypeError("in lambda"); }
    return 1;
  };
  try { print(`${f(true)}`); } catch (e) { if (e instanceof Error) { print(`caught ${e.name}`); } }
  try { print(`unreached ${await h}`); } catch (e) { if (e instanceof Error) { print(`caught late ${e.message}`); } }
  print("end");
}

export async function main(): Promise<void> {
  await plain();
  await exceptional();
}
