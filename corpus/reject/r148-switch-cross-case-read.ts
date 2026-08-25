// corpus: reject/r148-switch-cross-case-read
// purpose: Rejects a read of a local that a different switch case declares.
// exercises: switch-body-scope, cross-case-read
// questions: §67
// expected-error: S100 at the cross-case read

export function main(): void {
  const selected: i32 = 0;
  const caseValue: i32 = 99;
  switch (selected) {
    case 0:
      const caseValue: i32 = 1;
    case 1:
      print(`case1:${caseValue}`);
      break;
  }
}
