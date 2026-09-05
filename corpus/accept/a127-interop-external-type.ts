// corpus: accept/a127-interop-external-type
// interpreter: no — calls two synthetic native interop libraries
// purpose: Passes one opaque handle between two generated mirrors that share its external type spelling.
// exercises: external-mirror-type, two-header-binding, opaque-handle, foreign-call
// questions: Q13, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §48. interop.h owns SubDevice; external-device.h references it
// without emitting a second declaration. The value crosses into the second
// header and returns to an API from the first header.

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const same: SubDevice = subExternalDeviceIdentity(device);
  print(`${subExternalDeviceTag(same, 127)}`);
  subDeviceRelease(same);
}
