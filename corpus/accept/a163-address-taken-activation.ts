// corpus: accept/a163-address-taken-activation
// purpose: Keeps conditional boundary-aggregate temporaries alive until the activation ends after their addresses are taken.
// exercises: address-taken-liveness, nullable-boundary-member, foreign-call, script-call
// questions: §33.4, §68
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
function directForeignCall(useFirst: boolean): void {
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
      useFirst
        ? new SGPUProbeFragmentState(
            "conditional-first",
            constants,
            targets,
          )
        : new SGPUProbeFragmentState(
            "conditional-second",
            constants,
            targets,
          ),
    );
  let selector: u32 = 0;
  while (selector <= 22) {
    print(`${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

function storesNothing(
  descriptor: SGPUProbeFullRenderPipelineDescriptor,
): void {
  if (subProbeFullRenderPipelineCheck(descriptor, 2) > 1) {
    unreachable();
  }
}

function afterScriptCall(useFirst: boolean): void {
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
      useFirst
        ? new SGPUProbeFragmentState(
            "conditional-first",
            constants,
            targets,
          )
        : new SGPUProbeFragmentState(
            "conditional-second",
            constants,
            targets,
          ),
    );
  storesNothing(descriptor);
  let selector: u32 = 0;
  while (selector <= 22) {
    print(`${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

function reportFromCallee(
  descriptor: SGPUProbeFullRenderPipelineDescriptor,
): void {
  let selector: u32 = 0;
  while (selector <= 22) {
    print(`${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

function passToScriptCall(useFirst: boolean): void {
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
      useFirst
        ? new SGPUProbeFragmentState(
            "conditional-first",
            constants,
            targets,
          )
        : new SGPUProbeFragmentState(
            "conditional-second",
            constants,
            targets,
          ),
    );
  reportFromCallee(descriptor);
}

export function main(): void {
  directForeignCall(true);
  afterScriptCall(true);
  passToScriptCall(true);
  print("live_bytes=2977");
}
