// corpus: accept/a232-field-values-mixed
// purpose: Accepts a class that mixes initialized fields with fields the constructor assigns at its top level.
// observable: an initialized field keeps a nested reassignment, and a top-level assignment can follow a nested statement.
// exercises: class-field, field-initializer, constructor, conditional-assignment
// questions: compiler section 108
// tsc: accepts; js-comparable: yes
class Inner {
  value: i32 = 5;
}

class Holder {
  count: i32 = 0;
  label: string;
  inner: Inner;
  scale: i32;

  constructor(label: string, doubled: boolean) {
    this.label = label;
    if (doubled) {
      this.count = 2;
    }
    this.inner = new Inner();
    let scale: i32 = 1;
    for (let i: i32 = 0; i < 3; i++) {
      scale = scale * 2;
    }
    this.scale = scale;
  }
}

export function main(): void {
  const plain: Holder = new Holder("plain", false);
  const doubled: Holder = new Holder("doubled", true);
  print(`${plain.label} ${plain.count} ${plain.inner.value} ${plain.scale}`);
  print(`${doubled.label} ${doubled.count} ${doubled.inner.value} ${doubled.scale}`);
}
