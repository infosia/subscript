// corpus: accept/a29-interop-handle
// purpose: Exercises the opaque-handle create/retain/release lifecycle and observes completion.
// exercises: interop-handle, opaque-handle, lifecycle, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// Q16: self-created handle via subDeviceCreate(null) — the handle is not
// host-injected. The opaque handle lowers to a pointer-sized value with no
// visible layout; retain/release take it as that opaque pointer. create
// gives one reference, retain a second, and the two releases balance them
// (the final release frees), so the run completes without a trap. The
// lifecycle has no callback channel, so the observed result is the marker.

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  subDeviceRetain(device);
  subDeviceRelease(device);
  subDeviceRelease(device);
  print(`ok`);
}
