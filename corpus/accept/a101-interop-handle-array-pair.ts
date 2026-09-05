// corpus: accept/a101-interop-handle-array-pair
// interpreter: no — calls the synthetic native interop library
// purpose: Lowers a pipeline-layout-shaped string label and collapsed opaque-handle pair from script to C with pointer identity preserved.
// exercises: opaque-handle-array, struct-handle-pair, string-view-field, c-layout-scratch, zero-copy-slice, pointer-identity
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §31.1. The repeated first handle gives deterministic identity
// evidence without relying on process-global fixture state.

export function main(): void {
  const first: SubDevice = subDeviceCreate(null);
  const second: SubDevice = subDeviceCreate(null);
  const third: SubDevice = subDeviceCreate(null);
  const layouts: SubDevice[] = [first, second, third, first];
  const descriptor: SubProbePipelineLayoutDescriptor = new SubProbePipelineLayoutDescriptor(
    "r8-layout",
    layouts,
  );

  print(`${subProbePipelineLayoutCheck(descriptor, 0)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 1)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 2)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 3)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 4)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 5)}`);
  print(`${subProbePipelineLayoutCheck(descriptor, 6)}`);

  subDeviceRelease(first);
  subDeviceRelease(second);
  subDeviceRelease(third);
}
