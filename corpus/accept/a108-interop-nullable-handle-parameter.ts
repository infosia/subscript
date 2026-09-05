// corpus: accept/a108-interop-nullable-handle-parameter
// interpreter: no — calls the synthetic native interop library
// purpose: Passes a live opaque handle and null through a nullable handle parameter beside a leading non-null handle.
// exercises: nullable-handle-parameter, opaque-handle, null-to-NULL, foreign-call
// questions: Q13, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §35. The fixture distinguishes the leading encoder handle,
// a separate live group handle, and NULL without process-global state.

export function main(): void {
  const encoder: SubDevice = subDeviceCreate(null);
  const group: SubDevice = subDeviceCreate(null);

  print(`${subProbeSetBindGroupCheck(encoder, group)}`);
  print(`${subProbeSetBindGroupCheck(encoder, null)}`);

  subDeviceRelease(encoder);
  subDeviceRelease(group);
}
