// corpus: accept/a117-descriptor-literal-nullable-member
// purpose: Exercises descriptor object literals through nullable descriptor contexts.
// exercises: nullable-descriptor, member-initializer, argument, array-element, nesting, null-default
// questions: Q33, R17, C7

@Descriptor
class InnerDescriptor {
  a?: i32 = 7;
}

@Descriptor
class DefaultedNullableMember {
  m?: InnerDescriptor | null = null;
}

@Descriptor
class RequiredNullableMember {
  pick!: InnerDescriptor | null;
}

@Descriptor
class NestedNullableMember {
  value!: RequiredNullableMember;
}

function takeNullable(value: InnerDescriptor | null): void {
  print(`argument=${value !== null}`);
}

export function main(): void {
  const defaultedObject: DefaultedNullableMember = { m: {} };
  const defaultedNull: DefaultedNullableMember = { m: null };
  const defaultedOmitted: DefaultedNullableMember = {};
  print(
    `defaulted=${defaultedObject.m !== null},${defaultedNull.m !== null},${defaultedOmitted.m !== null}`,
  );

  const requiredObject: RequiredNullableMember = { pick: {} };
  const requiredNull: RequiredNullableMember = { pick: null };
  if (requiredObject.pick !== null) {
    print(`required-object=${requiredObject.pick.a}`);
  }
  print(`required-null=${requiredNull.pick !== null}`);

  takeNullable({});

  const nullableElements: (InnerDescriptor | null)[] = [{}, null];
  print(`array-element=${nullableElements[0] !== null},${nullableElements[1] !== null}`);

  const nested: NestedNullableMember = { value: { pick: {} } };
  if (nested.value.pick !== null) {
    print(`nested=${nested.value.pick.a}`);
  }

  const downstreamControl: RequiredNullableMember[] = [
    { pick: {} },
    { pick: null },
  ];
  print(
    `downstream-array=${downstreamControl[0].pick !== null},${downstreamControl[1].pick !== null}`,
  );
}
