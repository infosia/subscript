// corpus: reject/r12-general-union
// purpose: Rejects a non-null general union type.
// exercises: rejected-general-union, reference-class-field
// questions: none
// expected-error: unions are limited to T | null

class Choice {
  value: i32 | string;

  constructor(value: i32 | string) {
    this.value = value;
  }
}

export function main(): void {
  const choice: Choice = new Choice("text");
  print(`${choice.value}`);
}
