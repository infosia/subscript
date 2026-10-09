// corpus: accept/a362-interop-handle-field-kept
// interpreter: no — calls the synthetic native interop library
// purpose: Keeps a handle that a host-filled struct field delivers and uses it in later foreign calls.
// exercises: nullable-handle-field, null-readback, stored-wrapper, module-global, handle-identity
// questions: §142, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §142 rule 1: a handle field of a struct that the host
// fills transfers no ownership. The C checks report pointer identity,
// so the kept handle names the object that the host wrote.

class Binding {
  sampler: SubDevice;

  constructor(sampler: SubDevice) {
    this.sampler = sampler;
  }
}

let kept: Binding | null = null;
let keptView: SubDevice | null = null;

function deliver(device: SubDevice, slot: u32): SubProbeBindGroupEntry {
  const entry: SubProbeBindGroupEntry = new SubProbeBindGroupEntry(0, null, null, null);
  subProbeBindGroupEntryFill(entry, slot, device);
  return entry;
}

function adopt(sampler: SubDevice, view: SubDevice): void {
  const samplerEntry: SubProbeBindGroupEntry = deliver(sampler, 1);
  const field: SubDevice | null = samplerEntry.sampler;
  if (field !== null) {
    kept = new Binding(field);
  }
  keptView = deliver(view, 2).textureView;
  print(`handle-field:adopted=${samplerEntry.binding}`);
}

function report(encoder: SubDevice, sampler: SubDevice, view: SubDevice): void {
  const binding: Binding | null = kept;
  if (binding === null) {
    print("handle-field:missing");
    return;
  }
  print(`handle-field:encoder=${subProbeSetBindGroupCheck(encoder, encoder)}`);
  print(`handle-field:sampler=${subProbeSetBindGroupCheck(encoder, binding.sampler)}`);
  print(`handle-field:view=${subProbeSetBindGroupCheck(encoder, keptView)}`);
  print(`handle-field:same=${subProbeQueueSubmitCheck(binding.sampler, [sampler], 1)}`);
  print(`handle-field:other=${subProbeQueueSubmitCheck(binding.sampler, [view], 1)}`);
  const viewHandle: SubDevice | null = keptView;
  if (viewHandle !== null) {
    print(`handle-field:view-same=${subProbeQueueSubmitCheck(viewHandle, [view], 1)}`);
  }
}

export function main(): void {
  const encoder: SubDevice = subDeviceCreate(null);
  const sampler: SubDevice = subDeviceCreate(null);
  const view: SubDevice = subDeviceCreate(null);
  adopt(sampler, view);
  report(encoder, sampler, view);
  kept = null;
  keptView = null;
  subDeviceRelease(encoder);
  subDeviceRelease(sampler);
  subDeviceRelease(view);
}

// pin: 9963cfc3
// pin-dev-jit: Exit 0; output matches the golden.
// pin-c-aot: Exit 0; output matches the golden.
