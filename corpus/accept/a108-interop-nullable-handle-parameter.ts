// corpus: accept/a108-interop-nullable-handle-parameter
// purpose: Passes a live opaque handle and null through a nullable handle parameter beside a leading non-null handle.
// exercises: nullable-handle-parameter, opaque-handle, null-to-NULL, foreign-call
// questions: Q13, C7
// tsc: accepts
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
