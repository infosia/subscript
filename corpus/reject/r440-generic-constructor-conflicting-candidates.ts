// corpus: reject/r440-generic-constructor-conflicting-candidates
// purpose: Two different non-literal arguments give conflicting candidates for one constructor type parameter.
// exercises: generics, type-inference, constructor
// questions: compiler.md §197
// tsc: accepts
// expected-error: S100, cannot infer type parameter `T` of `Two`: conflicting candidates `i32` and `f64`
class Two<T> {
  a: T;
  b: T;
  constructor(a: T, b: T) {
    this.a = a;
    this.b = b;
  }
}

export function main(): void {
  const n: i32 = 1;
  const f: f64 = 2.5;
  const t = new Two(n, f);
  print(`${t.a}`);
}
