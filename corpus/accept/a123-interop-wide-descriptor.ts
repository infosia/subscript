// corpus: accept/a123-interop-wide-descriptor
// purpose: Pins scratch ownership for a wide render-pipeline-shaped descriptor combining nested pairs, by-value states, handles, and two reach-through pointer trees.
// exercises: recursive-boundary-lowering, wide-descriptor, breadth-depth-composition, pointer-bearing-array-elements, sibling-independent-scratch
// questions: Q13, C4, C7
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
function report(descriptor: SGPUProbeWideRenderPipelineDescriptor): void {
  print("wide-present");
  let selector: u32 = 0;
  while (selector <= 77) {
    print(`${subProbeWideRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);

  const vertexBuffers: SGPUProbeWidePairEntry[] = [
    new SGPUProbeWidePairEntry("vertex-a", [101, 102]),
    new SGPUProbeWidePairEntry("vertex-b", [103, 104]),
  ];
  const vertex: SGPUProbeWideVertexState =
    new SGPUProbeWideVertexState("vertex-main", vertexBuffers);
  const primitive: SGPUProbeWidePrimitiveState =
    new SGPUProbeWidePrimitiveState(201, 202);

  const depthConstants: SGPUProbeWidePairEntry[] = [
    new SGPUProbeWidePairEntry("depth-a", [301, 302]),
    new SGPUProbeWidePairEntry("depth-b", [303, 304]),
  ];
  const depthPayloadA: SGPUProbeWidePayload =
    new SGPUProbeWidePayload("depth-payload-a", [311, 312]);
  const depthPayloadB: SGPUProbeWidePayload =
    new SGPUProbeWidePayload("depth-payload-b", [313, 314]);
  const depthElements: SGPUProbeWidePointerElement[] = [
    new SGPUProbeWidePointerElement(321, depthPayloadA),
    new SGPUProbeWidePointerElement(322, depthPayloadB),
  ];
  const depthStencil: SGPUProbeWideDepthStencilState =
    new SGPUProbeWideDepthStencilState(depthConstants, depthElements);

  const multisample: SGPUProbeWideMultisampleState =
    new SGPUProbeWideMultisampleState(4, 4294967295, 1);

  const fragmentConstants: SGPUProbeWidePairEntry[] = [
    new SGPUProbeWidePairEntry("fragment-a", [401, 402]),
    new SGPUProbeWidePairEntry("fragment-b", [403, 404]),
  ];
  const fragmentPayloadA: SGPUProbeWidePayload =
    new SGPUProbeWidePayload("fragment-payload-a", [411, 412]);
  const fragmentPayloadB: SGPUProbeWidePayload =
    new SGPUProbeWidePayload("fragment-payload-b", [413, 414]);
  const fragmentElements: SGPUProbeWidePointerElement[] = [
    new SGPUProbeWidePointerElement(421, fragmentPayloadA),
    new SGPUProbeWidePointerElement(422, fragmentPayloadB),
  ];
  const fragment: SGPUProbeWideFragmentState =
    new SGPUProbeWideFragmentState(
      device,
      "fragment-main",
      fragmentConstants,
      fragmentElements,
    );

  const descriptor: SGPUProbeWideRenderPipelineDescriptor =
    new SGPUProbeWideRenderPipelineDescriptor(
      "wide-descriptor",
      device,
      vertex,
      primitive,
      depthStencil,
      multisample,
      fragment,
    );
  report(descriptor);
  subDeviceRelease(device);
}
