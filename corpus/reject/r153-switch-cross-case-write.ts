// corpus: reject/r153-switch-cross-case-write
// purpose: Rejects a write to a local that a different switch case declares.
// exercises: switch-body-scope, cross-case-write
// questions: §67
// tsc: accepts
// expected-error: S100 at the cross-case write

export function main(): void {
  const selected: i32 = 1;
  switch (selected) {
    case 0:
      let counter: i32 = 1;
      break;
    case 1:
      counter = 2;
      break;
    default:
      break;
  }
  print("end");
}
