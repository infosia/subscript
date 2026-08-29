// corpus: accept/a125-conditional-arm-narrowing
// purpose: Gives conditional arms the same null narrowing as if branches for references, handles, and converted boundary aggregates.
// exercises: conditional-expression, flow-narrowing, nullable-reference, nullable-handle, nullable-boundary-aggregate, branch-order
// questions: R19, R18, C7, Q13
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
class ReferenceValue {
  value: u32;

  constructor(value: u32) {
    this.value = value;
  }
}

function useReference(value: ReferenceValue): u32 {
  return value.value;
}

function referenceViaIf(value: ReferenceValue | null): u32 {
  if (value !== null) {
    return useReference(value);
  }
  return 0;
}

function referenceViaNotEqual(value: ReferenceValue | null): u32 {
  return value !== null ? useReference(value) : 0;
}

function referenceViaEqual(value: ReferenceValue | null): u32 {
  return value === null ? 0 : useReference(value);
}

function useHandle(value: SubDevice): u32 {
  return subProbeSetBindGroupCheck(value, value);
}

function handleViaIf(value: SubDevice | null): u32 {
  if (value !== null) {
    return useHandle(value);
  }
  return 0;
}

function handleViaNotEqual(value: SubDevice | null): u32 {
  return value !== null ? useHandle(value) : 0;
}

function handleViaEqual(value: SubDevice | null): u32 {
  return value === null ? 0 : useHandle(value);
}

class BlendSource {
  colorOperation: u32;
  alphaOperation: u32;

  constructor(colorOperation: u32, alphaOperation: u32) {
    this.colorOperation = colorOperation;
    this.alphaOperation = alphaOperation;
  }
}

function toBlend(source: BlendSource): SGPUProbeBlendState {
  return new SGPUProbeBlendState(
    source.colorOperation,
    source.alphaOperation,
  );
}

function boundaryViaIf(
  source: BlendSource | null,
  format: u32,
): SGPUProbeColorTargetState {
  if (source !== null) {
    return new SGPUProbeColorTargetState(format, toBlend(source), 1);
  }
  return new SGPUProbeColorTargetState(format, null, 1);
}

function boundaryViaNotEqual(
  source: BlendSource | null,
  format: u32,
): SGPUProbeColorTargetState {
  return new SGPUProbeColorTargetState(
    format,
    source !== null ? toBlend(source) : null,
    1,
  );
}

function boundaryViaEqual(
  source: BlendSource | null,
  format: u32,
): SGPUProbeColorTargetState {
  return new SGPUProbeColorTargetState(
    format,
    source === null ? null : toBlend(source),
    1,
  );
}

function reportBoundary(
  label: string,
  target: SGPUProbeColorTargetState,
): void {
  const fragment: SGPUProbeFragmentState = new SGPUProbeFragmentState(
    "conditional-arm",
    [],
    [target],
  );
  const descriptor: SGPUProbeFullRenderPipelineDescriptor =
    new SGPUProbeFullRenderPipelineDescriptor(label, fragment);
  print(
    `${label}=${subProbeFullRenderPipelineCheck(descriptor, 14)}:${subProbeFullRenderPipelineCheck(descriptor, 15)}:${subProbeFullRenderPipelineCheck(descriptor, 16)}`,
  );
}

export function main(): void {
  const reference: ReferenceValue = new ReferenceValue(11);
  print(`reference-if-value=${referenceViaIf(reference)}`);
  print(`reference-if-null=${referenceViaIf(null)}`);
  print(`reference-ne-value=${referenceViaNotEqual(reference)}`);
  print(`reference-ne-null=${referenceViaNotEqual(null)}`);
  print(`reference-eq-value=${referenceViaEqual(reference)}`);
  print(`reference-eq-null=${referenceViaEqual(null)}`);

  const handle: SubDevice = subDeviceCreate(null);
  print(`handle-if-value=${handleViaIf(handle)}`);
  print(`handle-if-null=${handleViaIf(null)}`);
  print(`handle-ne-value=${handleViaNotEqual(handle)}`);
  print(`handle-ne-null=${handleViaNotEqual(null)}`);
  print(`handle-eq-value=${handleViaEqual(handle)}`);
  print(`handle-eq-null=${handleViaEqual(null)}`);

  const blend: BlendSource = new BlendSource(31, 47);
  reportBoundary("boundary-if-value", boundaryViaIf(blend, 101));
  reportBoundary("boundary-if-null", boundaryViaIf(null, 102));
  reportBoundary("boundary-ne-value", boundaryViaNotEqual(blend, 103));
  reportBoundary("boundary-ne-null", boundaryViaNotEqual(null, 104));
  reportBoundary("boundary-eq-value", boundaryViaEqual(blend, 105));
  reportBoundary("boundary-eq-null", boundaryViaEqual(null, 106));

  subDeviceRelease(handle);
}
