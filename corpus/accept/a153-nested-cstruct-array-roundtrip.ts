// corpus: accept/a153-nested-cstruct-array-roundtrip
// purpose: Round-trips a nested CStruct value through a dynamic array without aliasing either value copy.
// exercises: CStruct, nested-value-class, dynamic-array, indexed-read, indexed-write, value-copy
// questions: §68
// tsc: accepts
@CStruct
class NestedValue {
  value: i32 = 0;

  constructor(value: i32) {
    this.value = value;
  }
}

@CStruct
class ArrayValue {
  nested: NestedValue = new NestedValue(0);
  tag: i32 = 0;

  constructor(nested: NestedValue, tag: i32) {
    this.nested = nested;
    this.tag = tag;
  }
}

export function main(): void {
  const values: ArrayValue[] = [
    new ArrayValue(new NestedValue(1), 2),
    new ArrayValue(new NestedValue(3), 4),
  ];

  const first: ArrayValue = values[0];
  print(`first=${first.nested.value},${first.tag}`);

  values[1] = first;
  values[1].nested.value += 10;
  const second: ArrayValue = values[1];
  print(`second=${second.nested.value},${second.tag}`);

  const firstAgain: ArrayValue = values[0];
  print(`first-again=${firstAgain.nested.value},${firstAgain.tag}`);
}
