// corpus: reject/r126-this-in-field-init
// purpose: Rejects reading this from a field initializer.
// exercises: field-initializer, this-binding
// questions: §57, R27
// expected-error: `this` is only available in constructors and methods

class InvalidInitializer {
  tag: i32 = 2;
  value: i32 = this.tag + 1;
}

export function main(): void {
  const value: InvalidInitializer = new InvalidInitializer();
  print(`${value.value}`);
}
