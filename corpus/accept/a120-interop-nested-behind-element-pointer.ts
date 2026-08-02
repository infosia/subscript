// corpus: accept/a120-interop-nested-behind-element-pointer
// purpose: Pins nested component structs behind a nullable blend pointer inside a scratch-lowered target-element array.
// exercises: recursive-boundary-lowering, nullable-struct-pointer, pointer-in-array-element, nested-behind-pointer, mixed-depth-scratch
// questions: Q13, C4, C7

function report(
  name: string,
  descriptor: SGPUProbeNestedRenderPipelineDescriptor,
  last: u32,
): void {
  print(name);
  let selector: u32 = 0;
  while (selector <= last) {
    print(`${subProbeFullRenderPipelineWithNestedBlendCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  const withoutFragment: SGPUProbeNestedRenderPipelineDescriptor =
    new SGPUProbeNestedRenderPipelineDescriptor("nested-absent", null);
  report("absent", withoutFragment, 3);

  const module: SubDevice = subDeviceCreate(null);
  const emptyConstants: SGPUProbeConstantEntry[] = [];
  const emptyTargets: SGPUProbeNestedColorTargetState[] = [];
  const emptyFragment: SGPUProbeNestedFragmentState =
    new SGPUProbeNestedFragmentState(
      module,
      "nested_empty",
      emptyConstants,
      emptyTargets,
    );
  const empty: SGPUProbeNestedRenderPipelineDescriptor =
    new SGPUProbeNestedRenderPipelineDescriptor("nested-empty", emptyFragment);
  report("empty", empty, 7);

  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("gamma", 375),
    new SGPUProbeConstantEntry("delta", 625),
  ];
  const color: SGPUProbeNestedBlendComponent =
    new SGPUProbeNestedBlendComponent(12, 34, 56);
  const alpha: SGPUProbeNestedBlendComponent =
    new SGPUProbeNestedBlendComponent(78, 90, 123);
  const blend: SGPUProbeNestedBlendState =
    new SGPUProbeNestedBlendState(color, alpha);
  const targets: SGPUProbeNestedColorTargetState[] = [
    new SGPUProbeNestedColorTargetState(101, null, 15),
    new SGPUProbeNestedColorTargetState(202, blend, 240),
  ];
  const fullFragment: SGPUProbeNestedFragmentState =
    new SGPUProbeNestedFragmentState(
      module,
      "nested_full",
      constants,
      targets,
    );
  const full: SGPUProbeNestedRenderPipelineDescriptor =
    new SGPUProbeNestedRenderPipelineDescriptor("nested-full", fullFragment);
  report("full", full, 31);

  subDeviceRelease(module);
}
