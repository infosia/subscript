// corpus: accept/a169-managed-boundary-box
// interpreter: no — calls the synthetic native interop library; the instruction-level box test is interpreted separately
// purpose: Keeps nullable boundary aggregates valid after global, return, array, and recursive-chain escapes.
// exercises: nullable-boundary-box, module-global, return, array-element, recursive-boundary-lowering
// questions: §33.5, §68
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
class Holder {
  descriptor: SGPUProbeFullRenderPipelineDescriptor;

  constructor(descriptor: SGPUProbeFullRenderPipelineDescriptor) {
    this.descriptor = descriptor;
  }
}

let stored: Holder | null = null;

function makeDescriptor(
  label: string,
  entryPoint: string,
): SGPUProbeFullRenderPipelineDescriptor {
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
  return new SGPUProbeFullRenderPipelineDescriptor(
    label,
    new SGPUProbeFragmentState(entryPoint, constants, targets),
  );
}

function report(
  tag: string,
  descriptor: SGPUProbeFullRenderPipelineDescriptor,
): void {
  let selector: u32 = 0;
  while (selector <= 22) {
    print(`${tag}:${selector}:${subProbeFullRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

function setup(): void {
  stored = new Holder(makeDescriptor("global", "global-fragment"));
}

function returnedDescriptor(): SGPUProbeFullRenderPipelineDescriptor {
  return makeDescriptor("returned", "returned-fragment");
}

function descriptorArray(): SGPUProbeFullRenderPipelineDescriptor[] {
  return [makeDescriptor("array", "array-fragment")];
}

function reportArray(
  descriptors: SGPUProbeFullRenderPipelineDescriptor[],
): void {
  report("array", descriptors[0]);
}

function buildChain(): SubChainExtA {
  const tail: SubChainExtA = new SubChainExtA(
    new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null),
    2.5,
    4,
  );
  return new SubChainExtA(
    new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, tail.header),
    7.75,
    5,
  );
}

function reportChain(): void {
  const head: SubChainExtA = buildChain();
  print(`chain:${subChainPayloadValue(head.header)}`);
}

export function main(): void {
  setup();
  const current: Holder | null = stored;
  if (current !== null) {
    report("global", current.descriptor);
  }

  const returned: SGPUProbeFullRenderPipelineDescriptor =
    returnedDescriptor();
  report("returned", returned);

  const descriptors: SGPUProbeFullRenderPipelineDescriptor[] =
    descriptorArray();
  reportArray(descriptors);
  reportChain();
}
