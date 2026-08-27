// corpus: reject/r147-static-accessor
// purpose: Rejects a static accessor with its specific diagnostic.
// exercises: static-accessor
// questions: R37
// tsc: accepts
// expected-error: S100 at the static accessor

class Value {
  static get current(): i32 {
    return 1;
  }
}

export function main(): void {}
