// corpus: reject/r193-using-value-class-hook
// purpose: Rejects a disposal hook on a value class.
// exercises: using-declaration, nullable-reference, symbol-dispose
// questions: §97, §60
// tsc: accepts
// expected-error: S100: value classes cannot declare Symbol.dispose
@CStruct
class Resource {
  [Symbol.dispose](): void {}
}
export function main(): void {}
