// corpus: reject/r133-using-without-dispose
// purpose: Rejects a using initializer whose class has no disposal hook.
// exercises: using-declaration, missing-symbol-dispose
// questions: §60, R31
// tsc: rejects TS2850
// expected-error: a using initializer class declares Symbol.dispose
class PlainResource {}

export function main(): void {
  using resource = new PlainResource();
}
