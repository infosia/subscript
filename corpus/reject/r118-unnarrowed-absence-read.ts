// corpus: reject/r118-unnarrowed-absence-read
// purpose: Rejects reading an absence-capable descriptor member without presence narrowing.
// exercises: descriptor-member, Q32 alias, absence-read, presence-narrowing
// questions: Q33, Q32, R16, C7
// tsc-clean-standalone: exit 0 verified with node_modules/.bin/tsc --noEmit --strict --target es2022 --lib es2022 corpus/reject/r118-unnarrowed-absence-read.ts prelude/lang.d.ts; stock TypeScript permits optional values in template positions.
// expected-error: S100 at the unnarrowed member read

type CompareFunction = "never" | "less" | "equal";

@Descriptor
class SamplerDescriptor {
  compare?: CompareFunction;
}

export function main(): void {
  const sampler: SamplerDescriptor = {};
  print(`compare=${sampler.compare}`);
}
