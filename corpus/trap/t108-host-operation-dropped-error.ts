// corpus: trap/t108-host-operation-dropped-error
// interpreter: no — calls the synthetic native interop library
// purpose: An Error completion traps after the last script holder ends.
// exercises: host-completion, dropped-handle, uncaught-exception
// questions: compiler.md §178, C8, Q13
// tsc: accepts
// js-comparable: no C8 Q13: Native completion handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the completion start call; message Error: host failure

async function drop(device: SubDevice, observe: boolean): Promise<void> {
  const job = subCompletionI32(device, 70);
  if (observe) { await job; }
}
export async function main(): Promise<void> {
  const device = subDeviceCreate(null);
  subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));
  await drop(device, false);
  subCompletionPump(device, 70, 1);
  print("after");
}

// pin: 09a1a889
// pin-dev-jit: Rejected S100: unknown provenance record kind `completion`; no stdout.
// pin-c-aot: Rejected S100: unknown provenance record kind `completion`; no stdout.
// pin-interpreter: no — calls the synthetic native interop library.
// tsc-version: TypeScript 5.9.2; exit 0.
