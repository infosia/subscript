// corpus: accept/a233-this-in-the-assignment-prefix
// purpose: Accepts the two `this` forms the assignment prefix permits, and every use after the prefix ends.
// observable: the constructor reads an initialized field, reads a field an earlier top-level statement assigned, then calls a method and passes `this`.
// exercises: class-field, constructor, this-read, this-method-call, this-argument
// questions: compiler section 108
// tsc: accepts; js-comparable: yes
class Inner {
  value: i32 = 3;
}

class Holder {
  count: i32 = 1;
  first: Inner;
  inner: Inner;

  constructor() {
    this.count = this.count + 1;
    this.first = new Inner();
    this.inner = this.first;
    this.show();
    describe(this);
  }

  show(): void {
    print(`${this.inner.value} ${this.count}`);
  }
}

function describe(holder: Holder): void {
  print(`${holder.first.value}`);
}

export function main(): void {
  const holder: Holder = new Holder();
  holder.show();
}
