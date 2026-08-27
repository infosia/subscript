// corpus: reject/r137-descriptor-align
// purpose: Rejects alignment options on a descriptor class.
// exercises: Descriptor, decorator-options
// questions: R33, Q33
// tsc: rejects TS1238, TS2554
// expected-error: Descriptor does not accept alignment options
@Descriptor({ align: 16 })
class InvalidDescriptor {
  value!: f32;
}

export function main(): void {}
