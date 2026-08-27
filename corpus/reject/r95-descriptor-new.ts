// corpus: reject/r95-descriptor-new
// purpose: Rejects `new` construction of a literal-only descriptor class.
// exercises: descriptor-class, new-expression, literal-only-construction
// questions: Q33
// tsc: accepts
// expected-error: S100 at the `new` expression

@Descriptor
class NewDescriptor {
  value?: i32 = 1;
}

export function main(): void {
  const descriptor: NewDescriptor = new NewDescriptor();
  print(`${descriptor.value}`);
}
