// corpus: reject/r195-using-nullable-member
// purpose: Rejects a member read without null narrowing.
// exercises: using-declaration, nullable-reference, symbol-dispose
// questions: §97, §60
// tsc: rejects TS18047
// expected-error: S011: narrow the nullable binding before member access
class Resource {
  label: string = "resource";
  [Symbol.dispose](): void {}
}
function maybe(): Resource | null { return new Resource(); }
export function main(): void {
  using resource = maybe();
  print(resource.label);
}
