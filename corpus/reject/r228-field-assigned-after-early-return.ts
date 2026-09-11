// corpus: reject/r228-field-assigned-after-early-return
// purpose: Rejects a field whose only top-level assignment stands after a constructor `return`.
// exercises: class-field, constructor, early-return
// questions: compiler section 108
// tsc: rejects TS2564
// expected-error: S100 at the field
class Inner {
  value: i32 = 3;
}

class Holder {
  inner: Inner;

  constructor(flag: boolean) {
    if (flag) {
      return;
    }
    this.inner = new Inner();
  }
}

export function main(): void {
  const holder: Holder = new Holder(true);
  print(`${holder.inner.value}`);
}
