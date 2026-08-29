// corpus: reject/r168-static-initializer-transitive-read
// purpose: Rejects a static initializer that reaches a later module binding.
// exercises: static-field, module-initializer, transitive-global-read
// questions: §67, §71
// tsc: accepts
// expected-error: S100 at the static initializer
class Config {
  static value: i32 = readLater();
}

function readLater(): i32 {
  return later;
}

const later: i32 = 4;

export function main(): void {}
