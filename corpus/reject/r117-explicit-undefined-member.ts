// corpus: reject/r117-explicit-undefined-member
// purpose: Rejects explicit undefined for an absence-capable descriptor member; absence is omission-only.
// exercises: descriptor-member, Q32 alias, explicit-undefined
// questions: Q33, Q32, R16, C7
// tsc: accepts
// expected-error: S012 at the explicit undefined member value

type CompareFunction = "never" | "less" | "equal";

@Descriptor
class SamplerDescriptor {
  compare?: CompareFunction;
}

export function main(): void {
  const sampler: SamplerDescriptor = { compare: undefined };
  print("unreachable");
}
