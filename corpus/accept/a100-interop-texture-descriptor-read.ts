// corpus: accept/a100-interop-texture-descriptor-read
// purpose: Copies a C-filled embedded extent back beside string materialization while preserving the collapsed pair's script array handle.
// exercises: string-view-field, nested-boundary-aggregate, struct-enum-pair, c-layout-scratch, aggregate-copy-back, view-copy-in
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
// compiler.md §30.1 read direction. The C filler replaces the string view,
// embedded extent, and trailing scalars. It leaves the count/pointer pair
// alone; copy-back must retain the original language array handle.

export function main(): void {
  const formats: SGPUProbeFormat[] = [
    SGPUProbeFormat.SGPU_PROBE_FORMAT_RGBA8,
    SGPUProbeFormat.SGPU_PROBE_FORMAT_BGRA8,
  ];
  const descriptor: SGPUProbeTextureDescriptor = new SGPUProbeTextureDescriptor(
    "before",
    new SGPUProbeExtent3D(1, 2, 3),
    formats,
    SGPUProbeFormat.SGPU_PROBE_FORMAT_RGBA8,
    1,
    1,
    1,
    1,
  );
  subProbeTextureDescriptorFill(descriptor);
  Context.collect();
  print(descriptor.label);
  print(`${descriptor.extent.width}`);
  print(`${descriptor.extent.height}`);
  print(`${descriptor.extent.depthOrArrayLayers}`);
  print(`${descriptor.format}`);
  print(`${descriptor.mipLevelCount}`);
  print(`${descriptor.sampleCount}`);
  print(`${descriptor.dimension}`);
  print(`${descriptor.usage}`);
  print(`${descriptor.viewFormats.length}`);
  print(`${descriptor.viewFormats[0]}`);
  print(`${descriptor.viewFormats[1]}`);
}
