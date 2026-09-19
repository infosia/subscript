// corpus: accept/a248-registration-script-reference-kept
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: A script reference kept after the release keeps the object across a collection.
// exercises: interop-callback, callback-registration-explicit-end, explicit-collection
// questions: Q7, Q13, Q16
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
class Kept {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const kept: Kept = new Kept(31);
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as Kept;
        print(`fired ${state.value + message.length}`);
      }
    },
    kept,
    null,
  );

  // The start completes and ends the registration before it returns, so
  // nothing roots the object after this call.
  subRequestStart(device, 1, 1, info);
  print(`released ${subRequestReleaseCount(device)}`);

  // §111 rule 7: release removes a root and frees nothing. This script
  // reference is a root of its own.
  Context.collect();
  print(`kept ${kept.value}`);
  subDeviceRelease(device);
}
