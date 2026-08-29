// corpus: reject/r166-static-member-through-instance
// purpose: Rejects a static member read through an instance.
// exercises: static-field, instance-access
// questions: §71
// tsc: rejects TS2576
// expected-error: S100 at the member read
class C {
  static value: i32 = 1;
}

export function main(): void {
  const item: C = new C();
  print(`${item.value}`);
}
