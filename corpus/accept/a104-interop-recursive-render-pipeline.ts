// corpus: accept/a104-interop-recursive-render-pipeline
// purpose: Composes recursive lowering through an embedded vertex state, a buffer-layout pair, and each layout's scalar attribute pair.
// exercises: recursive-boundary-lowering, render-pipeline-depth-chain, struct-element-scratch-array, nested-collapsed-pair, deepest-evidence
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const firstAttributes: SGPUProbeVertexAttribute[] = [
    new SGPUProbeVertexAttribute(1, 11, 64),
  ];
  const secondAttributes: SGPUProbeVertexAttribute[] = [
    new SGPUProbeVertexAttribute(2, 29, 128),
    new SGPUProbeVertexAttribute(3, 47, 256),
  ];
  const buffers: SGPUProbeVertexBufferLayout[] = [
    new SGPUProbeVertexBufferLayout(32, 0, firstAttributes),
    new SGPUProbeVertexBufferLayout(48, 1, secondAttributes),
  ];
  const vertex: SGPUProbeVertexState = new SGPUProbeVertexState(77, buffers);
  const descriptor: SGPUProbeRenderPipelineDescriptor = new SGPUProbeRenderPipelineDescriptor(
    "r9-render",
    vertex,
    9,
  );
  print(`${subProbeRenderPipelineCheck(descriptor, 0)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 1)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 2)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 3)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 4)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 5)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 6)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 7)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 8)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 9)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 10)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 11)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 12)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 13)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 14)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 15)}`);
  print(`${subProbeRenderPipelineCheck(descriptor, 16)}`);
}
