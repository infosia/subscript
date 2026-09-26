// corpus: trap/t65-exception-leaves-host-callback
// interpreter: no — calls the synthetic native interop library
// purpose: An exception that leaves a script callback of a host C function becomes the uncaught-exception trap at the trampoline, so no exception crosses a C frame.
// exercises: throw, interop-callback, callback-registration-explicit-end, uncaught-exception, array-for-each, try-catch
// questions: Q9, Q13
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in the callback
export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      print(`fire ${message.length}`);
      throw new Error("callback failed");
    },
    null,
    null,
  );
  const payloads: u32[] = [2];
  try {
    // The forEach call is a raise site with a handler, so an exception
    // that crossed the C frame would reach this catch.
    payloads.forEach((payload: u32): void => {
      subRequestStart(device, payload, 1, info);
      print("started");
    });
  } catch {
    print("caught");
  }
  print("unreached");
}
