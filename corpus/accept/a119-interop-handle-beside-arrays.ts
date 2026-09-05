// corpus: accept/a119-interop-handle-beside-arrays
// interpreter: no — calls the synthetic native interop library
// purpose: Pins a nullable scalar handle beside a string and two arrays in a scratch-lowered fragment reached through a nullable pointer member.
// exercises: recursive-boundary-lowering, nullable-struct-pointer, nullable-handle-field, handle-beside-arrays, mixed-depth-scratch
// questions: Q13, C4, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
function report(
  name: string,
  descriptor: SGPUProbeHandleRenderPipelineDescriptor,
  last: u32,
): void {
  print(name);
  let selector: u32 = 0;
  while (selector <= last) {
    print(`${subProbeFullRenderPipelineWithHandleCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  const withoutFragment: SGPUProbeHandleRenderPipelineDescriptor =
    new SGPUProbeHandleRenderPipelineDescriptor("obs3-absent", null);
  report("absent", withoutFragment, 3);

  const module: SubDevice = subDeviceCreate(null);
  const emptyConstants: SGPUProbeConstantEntry[] = [];
  const emptyTargets: SGPUProbeColorTargetState[] = [];

  const handleEmptyFragment: SGPUProbeHandleFragmentState =
    new SGPUProbeHandleFragmentState(
      module,
      "handle_empty",
      emptyConstants,
      emptyTargets,
    );
  const handleEmpty: SGPUProbeHandleRenderPipelineDescriptor =
    new SGPUProbeHandleRenderPipelineDescriptor("obs3-handle-empty", handleEmptyFragment);
  report("handle-empty", handleEmpty, 7);

  const nullEmptyFragment: SGPUProbeHandleFragmentState =
    new SGPUProbeHandleFragmentState(
      null,
      "null_empty",
      emptyConstants,
      emptyTargets,
    );
  const nullEmpty: SGPUProbeHandleRenderPipelineDescriptor =
    new SGPUProbeHandleRenderPipelineDescriptor("obs3-null-empty", nullEmptyFragment);
  report("null-empty", nullEmpty, 7);

  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("gamma", 375),
    new SGPUProbeConstantEntry("delta", 625),
  ];
  const blend: SGPUProbeBlendState = new SGPUProbeBlendState(12, 34);
  const targets: SGPUProbeColorTargetState[] = [
    new SGPUProbeColorTargetState(101, null, 15),
    new SGPUProbeColorTargetState(202, blend, 240),
  ];

  const handleFullFragment: SGPUProbeHandleFragmentState =
    new SGPUProbeHandleFragmentState(module, "handle_full", constants, targets);
  const handleFull: SGPUProbeHandleRenderPipelineDescriptor =
    new SGPUProbeHandleRenderPipelineDescriptor("obs3-handle-full", handleFullFragment);
  report("handle-full", handleFull, 23);

  const nullFullFragment: SGPUProbeHandleFragmentState =
    new SGPUProbeHandleFragmentState(null, "null_full", constants, targets);
  const nullFull: SGPUProbeHandleRenderPipelineDescriptor =
    new SGPUProbeHandleRenderPipelineDescriptor("obs3-null-full", nullFullFragment);
  report("null-full", nullFull, 23);

  subDeviceRelease(module);
}
