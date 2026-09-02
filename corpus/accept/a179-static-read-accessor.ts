// corpus: accept/a179-static-read-accessor
// purpose: Runs a static read accessor without a write accessor.
// observable: Two reads expose the static field value before and after a direct write.
// exercises: static-accessor, static-field, read-only-accessor
// questions: R39.8, §82.5, C12
// tsc: accepts; js-comparable: yes

class C {
  static value: i32 = 3;

  static get name(): i32 {
    return C.value;
  }
}

export function main(): void {
  print(`${C.name}`);
  C.value = 8;
  print(`${C.name}`);
}
