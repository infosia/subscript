// corpus: reject/r93-descriptor-optional-without-default
// purpose: Rejects a descriptor optional member that has no default.
// exercises: descriptor-member, optional-without-default
// questions: Q33, C7
// expected-error: S012 at the optional member

@Descriptor
class InvalidDescriptor {
  value?: i32;
}

export function main(): void {}
