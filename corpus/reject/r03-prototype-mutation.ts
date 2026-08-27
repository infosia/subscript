// corpus: reject/r03-prototype-mutation
// purpose: Rejects assignment through a class prototype.
// exercises: rejected-prototype-mutation
// questions: none
// tsc: accepts
// expected-error: no prototype mutation
class Greeter {
  message: string = "hello";
}

Greeter.prototype.message = "changed";

export function main(): void {
  print("prototype");
}
