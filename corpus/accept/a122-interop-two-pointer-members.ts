// corpus: accept/a122-interop-two-pointer-members
// purpose: Pins independent scratch storage for two simultaneously-present reach-through pointer members separated by a varying by-value aggregate.
// exercises: recursive-boundary-lowering, two-reach-through-pointers, nested-aggregate, array-pair, sibling-independent-scratch
// questions: Q13, C4, C7

function report(
  name: string,
  descriptor: SGPUProbeBreadthRenderPipelineDescriptor,
): void {
  print(name);
  let selector: u32 = 0;
  while (selector <= 15) {
    print(`${subProbeBreadthRenderPipelineCheck(descriptor, selector)}`);
    selector = selector + 1;
  }
}

export function main(): void {
  const depthNested: SGPUProbeBreadthNestedState =
    new SGPUProbeBreadthNestedState(101, 102);
  const depthBiases: u32[] = [103, 104];
  const depthStencil: SGPUProbeBreadthDepthStencilState =
    new SGPUProbeBreadthDepthStencilState(depthNested, depthBiases);

  const fragmentNested: SGPUProbeBreadthNestedState =
    new SGPUProbeBreadthNestedState(201, 202);
  const fragmentConstants: u32[] = [203, 204];
  const fragment: SGPUProbeBreadthFragmentState =
    new SGPUProbeBreadthFragmentState(fragmentNested, fragmentConstants);

  const zeroFields: SGPUProbeBreadthPrimitiveState =
    new SGPUProbeBreadthPrimitiveState(0, 0);
  const oneField: SGPUProbeBreadthPrimitiveState =
    new SGPUProbeBreadthPrimitiveState(301, 0);
  const twoFields: SGPUProbeBreadthPrimitiveState =
    new SGPUProbeBreadthPrimitiveState(301, 302);

  const bothZero: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-both-zero",
      depthStencil,
      zeroFields,
      fragment,
    );
  report("both-zero", bothZero);

  const bothOne: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-both-one",
      depthStencil,
      oneField,
      fragment,
    );
  report("both-one", bothOne);

  const bothTwo: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-both-two",
      depthStencil,
      twoFields,
      fragment,
    );
  report("both-two", bothTwo);

  const depthOnly: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-depth-only",
      depthStencil,
      twoFields,
      null,
    );
  report("depth-only", depthOnly);

  const fragmentOnly: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-fragment-only",
      null,
      twoFields,
      fragment,
    );
  report("fragment-only", fragmentOnly);

  const neither: SGPUProbeBreadthRenderPipelineDescriptor =
    new SGPUProbeBreadthRenderPipelineDescriptor(
      "breadth-neither",
      null,
      zeroFields,
      null,
    );
  report("neither", neither);
}
