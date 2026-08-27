// corpus: accept/a103-interop-recursive-compute-pipeline
// purpose: Recursively lowers an embedded compute state whose entry point is a string view.
// exercises: recursive-boundary-lowering, embedded-string-view, compute-pipeline-descriptor, c-layout-scratch
// questions: Q13, C4
// tsc: accepts
export function main(): void {
  const compute: SGPUProbeComputeState = new SGPUProbeComputeState(
    "main_cs",
    8,
    4,
    123456,
  );
  const descriptor: SGPUProbeComputePipelineDescriptor = new SGPUProbeComputePipelineDescriptor(
    "r9-compute",
    compute,
    165,
  );
  print(`${subProbeComputePipelineCheck(descriptor, 0)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 1)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 2)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 3)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 4)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 5)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 6)}`);
  print(`${subProbeComputePipelineCheck(descriptor, 7)}`);
}
