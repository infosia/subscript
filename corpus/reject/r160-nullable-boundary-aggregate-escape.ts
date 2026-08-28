// corpus: reject/r160-nullable-boundary-aggregate-escape
// purpose: Rejects a holder with a conditional nullable boundary aggregate that escapes through a module global.
// exercises: nullable-boundary-aggregate, module-global, activation-lifetime
// questions: §33.4, §68
// tsc: accepts
// expected-error: S015 at the reference-class field store
class Holder {
  descriptor: SGPUProbeFullRenderPipelineDescriptor;

  constructor(descriptor: SGPUProbeFullRenderPipelineDescriptor) {
    this.descriptor = descriptor;
  }
}

let stored: Holder | null = null;

function setup(useFirst: boolean): void {
  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("first", 125),
  ];
  const targets: SGPUProbeColorTargetState[] = [
    new SGPUProbeColorTargetState(
      404,
      new SGPUProbeBlendState(31, 47),
      204,
    ),
  ];
  stored = new Holder(
    new SGPUProbeFullRenderPipelineDescriptor(
      "global",
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
    ),
  );
}

export function main(): void {
  setup(true);
  const current: Holder | null = stored;
  if (current !== null) {
    const descriptor: SGPUProbeFullRenderPipelineDescriptor =
      current.descriptor;
    print(
      `${subProbeFullRenderPipelineCheck(descriptor, 3)}:${subProbeFullRenderPipelineCheck(descriptor, 4)}:${subProbeFullRenderPipelineCheck(descriptor, 5)}:${subProbeFullRenderPipelineCheck(descriptor, 12)}:${subProbeFullRenderPipelineCheck(descriptor, 15)}:${subProbeFullRenderPipelineCheck(descriptor, 16)}`,
    );
  }
}
