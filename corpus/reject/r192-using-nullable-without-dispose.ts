// corpus: reject/r192-using-nullable-without-dispose
// purpose: Rejects a nullable class without a disposal hook.
// exercises: using-declaration, nullable-reference, symbol-dispose
// questions: §97, §60
// tsc: rejects TS2850
// expected-error: S100: the class must declare Symbol.dispose
class Resource {}
function maybe(): Resource | null { return new Resource(); }
export function main(): void {
  using resource = maybe();
}
