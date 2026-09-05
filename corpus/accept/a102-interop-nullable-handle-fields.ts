// corpus: accept/a102-interop-nullable-handle-fields
// interpreter: no — calls the synthetic native interop library
// purpose: Round-trips one-of-three nullable opaque-handle fields through a bind-group-entry-shaped C record in both directions.
// exercises: nullable-handle-field, null-lowering, null-readback, boundary-struct-pointer, one-of-three
// questions: Q13, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §31.2. Direct field reads prove that C NULL becomes language
// null; the C checker independently proves that script null crossed as NULL.

function reportSetField(entry: SubProbeBindGroupEntry): void {
  if (entry.buffer !== null) {
    print("buffer");
  }
  if (entry.sampler !== null) {
    print("sampler");
  }
  if (entry.textureView !== null) {
    print("textureView");
  }
}

export function main(): void {
  const buffer: SubDevice = subDeviceCreate(null);
  const sampler: SubDevice = subDeviceCreate(null);
  const textureView: SubDevice = subDeviceCreate(null);

  const bufferEntry: SubProbeBindGroupEntry = new SubProbeBindGroupEntry(
    10,
    buffer,
    null,
    null,
  );
  const samplerEntry: SubProbeBindGroupEntry = new SubProbeBindGroupEntry(
    11,
    null,
    sampler,
    null,
  );
  const textureEntry: SubProbeBindGroupEntry = new SubProbeBindGroupEntry(
    12,
    null,
    null,
    textureView,
  );
  print(`${subProbeBindGroupEntryCheck(bufferEntry)}`);
  print(`${subProbeBindGroupEntryCheck(samplerEntry)}`);
  print(`${subProbeBindGroupEntryCheck(textureEntry)}`);

  const filled: SubProbeBindGroupEntry = new SubProbeBindGroupEntry(0, null, null, null);
  subProbeBindGroupEntryFill(filled, 0, buffer);
  print(`${filled.binding}:${subProbeBindGroupEntryCheck(filled)}`);
  reportSetField(filled);
  subProbeBindGroupEntryFill(filled, 1, sampler);
  print(`${filled.binding}:${subProbeBindGroupEntryCheck(filled)}`);
  reportSetField(filled);
  subProbeBindGroupEntryFill(filled, 2, textureView);
  print(`${filled.binding}:${subProbeBindGroupEntryCheck(filled)}`);
  reportSetField(filled);

  subDeviceRelease(buffer);
  subDeviceRelease(sampler);
  subDeviceRelease(textureView);
}
