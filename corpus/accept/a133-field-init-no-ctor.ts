// corpus: accept/a133-field-init-no-ctor
// purpose: Runs non-zero field initializers for value and reference classes without constructors.
// exercises: field-initializer, constructor-less-value-class, constructor-less-reference-class
// questions: §57, R27
// tsc: accepts
@CStruct
class ValueField {
  value: i32 = 37;
}

class ReferenceField {
  value: i32 = 41;
}

export function main(): void {
  const valueField: ValueField = new ValueField();
  const referenceField: ReferenceField = new ReferenceField();
  print(`value:${valueField.value}`);
  print(`reference:${referenceField.value}`);
}
