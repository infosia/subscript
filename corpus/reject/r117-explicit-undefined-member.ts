// corpus: reject/r117-explicit-undefined-member
// purpose: Rejects explicit undefined for an absence-capable descriptor member; absence is omission-only.
// exercises: descriptor-member, Q32 alias, explicit-undefined
// questions: Q33, Q32, R16, C7
// tsc-clean-standalone: exit 0 verified with node_modules/.bin/tsc --noEmit --strict --target es2022 --lib es2022 corpus/reject/r117-explicit-undefined-member.ts prelude/lang.d.ts; stock TypeScript accepts undefined for an optional member.
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
