// corpus: accept/a347-host-operation-completes
// interpreter: no — calls the synthetic native interop library
// purpose: Host completions support ordinary handle waits, errors, and collection.
// exercises: host-completion, shared-await, repeated-await, promise-all, collect, boundary-value
// questions: compiler.md §178, C8, Q13
// tsc: accepts
// js-comparable: no C8 Q13: Native completion and Context collection have no JavaScript shim.

async function read(job: Promise<i32>): Promise<i32> { return await job; }
async function fail(job: Promise<i32>): Promise<void> {
  try { print(`${await job}`); } catch (error) {
    if (error instanceof Error) { print(error.message); }
  }
}
export async function main(): Promise<void> {
  const device = subDeviceCreate(null);
  subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));
  const job = subCompletionI32(device, 11);
  const first = read(job);
  const second = read(job);
  Context.collect();
  print(`status ${subCompletionPump(device, 11, 0)}`);
  print(`${await first} ${await second} ${await job}`);
  print(`${await subCompletionImmediate(device, 22)}`);
  const a = subCompletionI32(device, 31);
  const b = subCompletionI32(device, 32);
  const all = Promise.all([a, b]);
  subCompletionPump(device, 31, 0);
  subCompletionPump(device, 32, 0);
  const values = await all;
  print(`${values[0]} ${values[1]}`);
  const bad = subCompletionI32(device, 40);
  const caught = fail(bad);
  subCompletionPump(device, 40, 1);
  await caught;
  const pair = subCompletionStruct(device, 50);
  subCompletionPump(device, 50, 0);
  const value = await pair;
  print(`${value.x} ${value.y}`);
  const empty = subCompletionVoid(device, 60);
  subCompletionPump(device, 60, 0);
  await empty;
  print("void");
  subDeviceRelease(device);
}

// pin: 09a1a889
// pin-dev-jit: Rejected S100: unknown provenance record kind `completion`; no stdout.
// pin-c-aot: Rejected S100: unknown provenance record kind `completion`; no stdout.
// pin-interpreter: no — calls the synthetic native interop library.
// tsc-version: TypeScript 5.9.2; exit 0.
