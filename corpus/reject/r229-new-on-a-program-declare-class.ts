// corpus: reject/r229-new-on-a-program-declare-class
// purpose: Rejects `new` on a `declare class` that a program file declares, because the declaration has no constructor body.
// exercises: declare-class, new, ambient-declaration
// questions: compiler section 108
// tsc: accepts
// expected-error: S100 at the `new`
class Inner {
  value: i32 = 3;
}

declare class Ext {
  inner: Inner;
}

export function main(): void {
  const ext: Ext = new Ext();
  print(`${ext.inner.value}`);
}
