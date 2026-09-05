// corpus: accept/a124-contextual-conditional
// interpreter: no — calls the synthetic native interop library for boundary-handle arms
// purpose: Exercises contextual conditional typing for nullable references, handles, and boundary aggregates in both branch orders.
// exercises: conditional-expression, contextual-typing, nullable-reference, nullable-handle, nullable-boundary-aggregate, branch-order
// questions: R18, C7, Q13
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
class RefValue {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

function referenceThen(flag: boolean): RefValue | null {
  return flag ? new RefValue(11) : null;
}

function nullThen(flag: boolean): RefValue | null {
  return flag ? null : new RefValue(22);
}

function reportReference(label: string, value: RefValue | null): void {
  if (value !== null) {
    print(`${label}=value:${value.value}`);
  } else {
    print(`${label}=null`);
  }
}

function reportBoundary(label: string, target: SGPUProbeColorTargetState): void {
  const targets: SGPUProbeColorTargetState[] = [target];
  const fragment: SGPUProbeFragmentState = new SGPUProbeFragmentState(
    "conditional",
    [],
    targets,
  );
  const descriptor: SGPUProbeFullRenderPipelineDescriptor =
    new SGPUProbeFullRenderPipelineDescriptor(label, fragment);
  print(
    `${label}=${subProbeFullRenderPipelineCheck(descriptor, 14)}:${subProbeFullRenderPipelineCheck(descriptor, 15)}:${subProbeFullRenderPipelineCheck(descriptor, 16)}`,
  );
}

export function main(): void {
  reportReference("reference-then-value", referenceThen(true));
  reportReference("reference-then-null", referenceThen(false));
  reportReference("null-then-null", nullThen(true));
  reportReference("null-then-value", nullThen(false));

  const encoder: SubDevice = subDeviceCreate(null);
  const group: SubDevice = subDeviceCreate(null);
  print(`${subProbeSetBindGroupCheck(encoder, true ? group : null)}`);
  print(`${subProbeSetBindGroupCheck(encoder, false ? group : null)}`);
  print(`${subProbeSetBindGroupCheck(encoder, true ? null : group)}`);
  print(`${subProbeSetBindGroupCheck(encoder, false ? null : group)}`);

  const blend: SGPUProbeBlendState = new SGPUProbeBlendState(31, 47);
  const aggregateThenValue: SGPUProbeColorTargetState =
    new SGPUProbeColorTargetState(101, true ? blend : null, 1);
  const aggregateThenNull: SGPUProbeColorTargetState =
    new SGPUProbeColorTargetState(102, false ? blend : null, 2);
  const nullThenNull: SGPUProbeColorTargetState =
    new SGPUProbeColorTargetState(103, true ? null : blend, 4);
  const nullThenValue: SGPUProbeColorTargetState =
    new SGPUProbeColorTargetState(104, false ? null : blend, 8);
  reportBoundary("aggregate-then-value", aggregateThenValue);
  reportBoundary("aggregate-then-null", aggregateThenNull);
  reportBoundary("null-then-null", nullThenNull);
  reportBoundary("null-then-value", nullThenValue);

  subDeviceRelease(encoder);
  subDeviceRelease(group);
}
