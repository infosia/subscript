// corpus: accept/a106-interop-recursive-struct-pointer-members
// purpose: Recursively lowers a render descriptor through nullable fragment and blend struct-pointer members.
// exercises: recursive-boundary-lowering, struct-pointer-members, nullable-fragment, nullable-blend, mixed-depth-scratch
// questions: Q13, C4

function report(descriptor: SGPUProbeFullRenderPipelineDescriptor, last: u32): void {
  let selector: u32 = 0;
  while (selector <= last) {
    print(`${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  // Null fragment: selector 2 proves NULL and selector 3 pins deterministic
  // handling of a read behind that null spelling.
  const withoutFragment: SGPUProbeFullRenderPipelineDescriptor =
    new SGPUProbeFullRenderPipelineDescriptor("r10-null", null);
  report(withoutFragment, 3);

  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("gamma", 375),
    new SGPUProbeConstantEntry("delta", 625),
  ];
  const blend: SGPUProbeBlendState = new SGPUProbeBlendState(12, 34);
  const targets: SGPUProbeColorTargetState[] = [
    // Null blend and non-null blend occur in the same recursively rebuilt
    // scratch array; selectors 14–16 and 19–21 distinguish them.
    new SGPUProbeColorTargetState(101, null, 15),
    new SGPUProbeColorTargetState(202, blend, 240),
  ];
  const fragment: SGPUProbeFragmentState = new SGPUProbeFragmentState(
    "fragment_main",
    constants,
    targets,
  );
  const withFragment: SGPUProbeFullRenderPipelineDescriptor =
    new SGPUProbeFullRenderPipelineDescriptor("r10-full", fragment);
  report(withFragment, 22);
}
