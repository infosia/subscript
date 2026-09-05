// corpus: accept/a99-interop-texture-descriptor-write
// interpreter: no — calls the synthetic native interop library
// purpose: Lowers a texture descriptor mixing a string view, embedded extent, enum array pair, and trailing scalars from script to C.
// exercises: string-view-field, nested-boundary-aggregate, struct-enum-pair, c-layout-scratch, zero-copy-slice, scalar-offsets
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §30. The checker observes every C component. In particular,
// the aggregate after the expanded string and the fields after the absorbed
// count/pointer pair prove the actual C offsets used by both tiers.

export function main(): void {
  const formats: SGPUProbeFormat[] = [
    SGPUProbeFormat.SGPU_PROBE_FORMAT_RGBA8,
    SGPUProbeFormat.SGPU_PROBE_FORMAT_BGRA8,
    SGPUProbeFormat.SGPU_PROBE_FORMAT_DEPTH24,
  ];
  const descriptor: SGPUProbeTextureDescriptor = new SGPUProbeTextureDescriptor(
    "r7-texture",
    new SGPUProbeExtent3D(640, 480, 6),
    formats,
    SGPUProbeFormat.SGPU_PROBE_FORMAT_BGRA8,
    7,
    4,
    2,
    165,
  );
  print(`${subProbeTextureDescriptorCheck(descriptor, 0)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 1)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 2)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 3)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 4)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 5)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 6)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 7)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 8)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 9)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 10)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 11)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 12)}`);
  print(`${subProbeTextureDescriptorCheck(descriptor, 13)}`);
}
