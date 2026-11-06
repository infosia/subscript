// corpus: reject/r488-user-call-unread-type-argument
// purpose: A type argument list on a call of a user method that is not generic is rejected, as on a built-in call.
// exercises: type-arguments, class-method
// questions: compiler.md §231.1 rule 1
// tsc: rejects TS2558
// expected-error: S100 at line 15, method `get` is not generic

class B {
  get(): i32 {
    return 1;
  }
}

function read(b: B): i32 {
  return b.get<i32>();
}

export function main(): void {
  print(`${read(new B())}`);
}
