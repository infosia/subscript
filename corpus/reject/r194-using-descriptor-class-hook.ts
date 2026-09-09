// corpus: reject/r194-using-descriptor-class-hook
// purpose: Rejects a disposal hook on a descriptor class.
// exercises: using-declaration, nullable-reference, symbol-dispose
// questions: §97, §60
// tsc: accepts
// expected-error: S100: descriptor classes cannot declare Symbol.dispose
@Descriptor
class Resource {
  [Symbol.dispose](): void {}
}
export function main(): void {}
