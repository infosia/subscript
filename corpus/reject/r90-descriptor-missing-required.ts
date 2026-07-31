// corpus: reject/r90-descriptor-missing-required
// purpose: Rejects a descriptor literal missing a required member.
// exercises: descriptor-literal, required-member
// questions: Q33
// expected-error: S100 at the constructing literal

@Descriptor
class RequiredDescriptor {
  value!: i32;
}

export function main(): void {
  const missing: RequiredDescriptor = {};
  print(`${missing.value}`);
}
