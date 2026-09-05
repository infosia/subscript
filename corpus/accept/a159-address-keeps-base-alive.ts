// corpus: accept/a159-address-keeps-base-alive
// interpreter: no — calls the synthetic native interop library
// purpose: Keeps a conditional boundary-aggregate temporary alive through a nullable member address.
// exercises: address-provenance, conditional-temporary, nullable-boundary-member, recursive-boundary-lowering
// questions: §33.4, §68
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
function report(descriptor: SGPUProbeFullRenderPipelineDescriptor): void {
  let selector: u32 = 0;
  while (selector <= 22) {
    print(`${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

function run(useFragment: boolean): void {
  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("first", 125),
    new SGPUProbeConstantEntry("second", 875),
  ];
  const targets: SGPUProbeColorTargetState[] = [
    new SGPUProbeColorTargetState(303, null, 51),
    new SGPUProbeColorTargetState(
      404,
      new SGPUProbeBlendState(56, 78),
      204,
    ),
  ];
  const descriptor: SGPUProbeFullRenderPipelineDescriptor =
    new SGPUProbeFullRenderPipelineDescriptor(
      "address-base",
      useFragment
        ? new SGPUProbeFragmentState(
            "conditional-first",
            constants,
            targets,
          )
        : new SGPUProbeFragmentState(
            "conditional-second",
            constants,
            targets,
          )
    );
  report(descriptor);
}

export function main(): void {
  run(true);
}
