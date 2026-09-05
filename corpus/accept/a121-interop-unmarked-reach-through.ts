// corpus: accept/a121-interop-unmarked-reach-through
// interpreter: no — calls the synthetic native interop library
// purpose: Pins shape-based recursive lowering through a count-less plain struct-pointer member inside an array element.
// exercises: recursive-boundary-lowering, unmarked-struct-pointer, pointer-in-array-element, enum-pointer-u64-layout, lowered-or-loud
// questions: Q13, C4, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
function report(
  name: string,
  descriptor: SGPUProbeUnmarkedRenderPipelineDescriptor,
  last: u32,
): void {
  print(name);
  let selector: u32 = 0;
  while (selector <= last) {
    print(`${subProbeFullRenderPipelineWithUnmarkedBlendCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  const absent: SGPUProbeUnmarkedRenderPipelineDescriptor =
    new SGPUProbeUnmarkedRenderPipelineDescriptor("unmarked-absent", null);
  report("absent", absent, 3);

  const module: SubDevice = subDeviceCreate(null);
  const emptyConstants: SGPUProbeConstantEntry[] = [];
  const emptyTargets: SGPUProbeUnmarkedColorTargetState[] = [];
  const emptyFragment: SGPUProbeUnmarkedFragmentState =
    new SGPUProbeUnmarkedFragmentState(
      module,
      "unmarked_empty",
      emptyConstants,
      emptyTargets,
    );
  const empty: SGPUProbeUnmarkedRenderPipelineDescriptor =
    new SGPUProbeUnmarkedRenderPipelineDescriptor("unmarked-empty", emptyFragment);
  report("empty", empty, 7);

  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("gamma", 375),
    new SGPUProbeConstantEntry("delta", 625),
  ];
  const blend: SGPUProbeUnmarkedBlendState =
    new SGPUProbeUnmarkedBlendState(12, 34);
  const targets: SGPUProbeUnmarkedColorTargetState[] = [
    new SGPUProbeUnmarkedColorTargetState(
      SGPUProbeUnmarkedTextureFormat.SGPU_PROBE_UNMARKED_TEXTURE_FORMAT_RGBA8,
      null,
      4294967311,
    ),
    new SGPUProbeUnmarkedColorTargetState(
      SGPUProbeUnmarkedTextureFormat.SGPU_PROBE_UNMARKED_TEXTURE_FORMAT_BGRA8,
      blend,
      8589934832,
    ),
  ];
  const fullFragment: SGPUProbeUnmarkedFragmentState =
    new SGPUProbeUnmarkedFragmentState(
      module,
      "unmarked_full",
      constants,
      targets,
    );
  const full: SGPUProbeUnmarkedRenderPipelineDescriptor =
    new SGPUProbeUnmarkedRenderPipelineDescriptor("unmarked-full", fullFragment);
  report("full", full, 23);

  subDeviceRelease(module);
}
