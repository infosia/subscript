// corpus: reject/r172-using-in-lambda
// purpose: Rejects a using declaration inside a lambda body.
// exercises: using-declaration, lambda-body, nested-declaration
// questions: §60, §76.3
// tsc: accepts
// expected-error: S100 at the using declaration
class Resource {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
  [Symbol.dispose](): void {
    print(`dispose ${this.id}`);
  }
}
export function main(): void {
  const f = (): i32 => {
    using r = new Resource(1);
    return r.id + 41;
  };
  print(`${f()}`);
}
