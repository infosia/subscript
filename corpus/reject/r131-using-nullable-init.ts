// corpus: reject/r131-using-nullable-init
// purpose: Rejects a nullable initializer in a using declaration.
// exercises: using-declaration, nullable-reference, symbol-dispose
// questions: §60, R31
// tsc: accepts
// expected-error: narrow a nullable value before a using declaration
class NullableResource {
  [Symbol.dispose](): void {}
}

function maybeResource(): NullableResource | null {
  return null;
}

export function main(): void {
  using resource = maybeResource();
}
