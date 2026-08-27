// corpus: accept/a118-absence-capable-member
// purpose: Exercises absence-capable Q32 alias members on descriptor literals and narrowed reads.
// exercises: descriptor-member, Q32 alias, omission, presence-narrowing, reserved-discriminant
// questions: Q33, Q32, R16, C7
// tsc: accepts
type CompareFunction = "never" | "less" | "equal";

@Descriptor
class SamplerDescriptor {
  compare?: CompareFunction;
  lodMaxClamp?: f32 = 32.0;
}

function inspect(label: string, sampler: SamplerDescriptor): void {
  if (sampler.compare !== undefined) {
    print(`${label}:present=${sampler.compare},lod=${sampler.lodMaxClamp}`);
  } else {
    print(`${label}:absent,lod=${sampler.lodMaxClamp}`);
  }

  if (sampler.compare === undefined) {
    print(`${label}:negative=absent`);
  } else {
    print(`${label}:negative=present:${sampler.compare}`);
  }
}

export function main(): void {
  const present: SamplerDescriptor = {
    compare: "less",
    lodMaxClamp: 4.0,
  };
  const absentWithOtherMembers: SamplerDescriptor = {
    lodMaxClamp: 8.0,
  };
  const empty: SamplerDescriptor = {};

  inspect("present", present);
  inspect("other", absentWithOtherMembers);
  inspect("empty", empty);
}
