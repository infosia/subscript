// corpus: accept/a354-completion-string-bytes
// interpreter: no — calls the synthetic native interop library
// purpose: Host completions copy text and bytes into Context values.
// exercises: host-completion, string, byte-array, promise-all, collect, error
// questions: compiler.md §184, C8, Q13
// tsc: accepts
// js-comparable: no C8 Q13: Native completion and Context collection have no JavaScript shim.

export async function main(): Promise<void> {
  const device = subDeviceCreate(null);
  subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));
  const text = subCompletionText(device, 0);
  const bytes = subCompletionBytes(device, 0);
  Context.collect();
  const s = await text;
  const b = await bytes;
  print(`${s}:${s.length}:${b.length}:${b[0]}:${b[1]}:${b[2]}`);
  const all = Promise.all([subCompletionText(device, 0), subCompletionText(device, 0)]);
  Context.collect();
  const values = await all;
  print(`${values[0]}:${values[1]}`);
  try { await subCompletionTextError(device); } catch (error) {
    if (error instanceof Error) { print(error.message); }
  }
  const emptyText = await subCompletionText(device, 1);
  const emptyBytes = await subCompletionBytes(device, 1);
  print(`empty:${emptyText.length}:${emptyBytes.length}`);
  subDeviceRelease(device);
}

// pin: da3d3133
// pin-dev-jit: Rejected S100: completion result string or u8[] is unsupported; no stdout.
// pin-c-aot: Rejected S100: completion result string or u8[] is unsupported; no stdout.
// pin-interpreter: no — calls the synthetic native interop library.
