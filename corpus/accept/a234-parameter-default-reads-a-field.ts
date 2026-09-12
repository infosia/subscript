// corpus: accept/a234-parameter-default-reads-a-field
// purpose: Evaluates a constructor parameter default after the field initializers.
// observable: the default of the absent argument reads the field an initializer filled, so the constructor stores 5.
// exercises: class-field, field-initializer, parameter-default, this-read
// questions: compiler section 57, compiler section 108
// tsc: accepts; js-comparable: yes
class Holder {
  count: i32 = 5;
  seen: i32;

  constructor(n: i32 = this.count) {
    this.seen = n;
  }
}

export function main(): void {
  const holder: Holder = new Holder();
  print(`${holder.seen} ${holder.count}`);
}
