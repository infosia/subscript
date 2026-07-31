// corpus: reject/r91-descriptor-excess-member
// purpose: Rejects an excess member under descriptor closed-property checking.
// exercises: descriptor-literal, closed-properties
// questions: Q33, C1
// expected-error: S004 at the excess member

@Descriptor
class ClosedDescriptor {
  value!: i32;
}

export function main(): void {
  const excess: ClosedDescriptor = { value: 1, extra: 2 };
  print(`${excess.value}`);
}
