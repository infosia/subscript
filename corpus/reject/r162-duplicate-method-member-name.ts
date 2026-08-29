// corpus: reject/r162-duplicate-method-member-name
// purpose: Rejects two methods with the same member name.
// exercises: class-member-namespace, duplicate-method-name
// questions: §67
// tsc: rejects TS2393
// expected-error: S100 at the second declaration
class C {
  x(): i32 {
    return 1;
  }

  x(): i32 {
    return 2;
  }
}

export function main(): void {
  const c: C = new C();
  print(`${c.x()}`);
}
