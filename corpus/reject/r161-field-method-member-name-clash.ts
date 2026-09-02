// corpus: reject/r161-field-method-member-name-clash
// purpose: Rejects a field and a method with the same member name.
// exercises: class-member-namespace, field-method-name-clash
// questions: §67
// tsc: rejects TS2300
// expected-error: S017 at the second declaration
class C {
  x: i32 = 1;
  x(): i32 {
    return 2;
  }
}

export function main(): void {
  const c: C = new C();
  print(`${c.x}`);
}
