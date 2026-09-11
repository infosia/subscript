// corpus: reject/r227-field-assigned-in-both-branches
// purpose: Rejects a field that both arms of a constructor conditional assign, because neither assignment is at the top level.
// exercises: class-field, constructor, conditional-assignment
// questions: compiler section 108
// tsc: accepts
// expected-error: S100 at the field
class Holder {
  value: i32;

  constructor(flag: boolean) {
    if (flag) {
      this.value = 1;
    } else {
      this.value = 2;
    }
  }
}

export function main(): void {
  const holder: Holder = new Holder(false);
  print(`${holder.value}`);
}
