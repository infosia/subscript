// corpus: reject/r163-duplicate-field-member-name
// purpose: Rejects two fields with the same member name.
// exercises: class-member-namespace, duplicate-field-name
// questions: §67
// tsc: rejects TS2300
// expected-error: S017 at the second declaration
class C {
  x: i32 = 1;
  x: i32 = 2;
}

export function main(): void {
  const c: C = new C();
  print(`${c.x}`);
}
