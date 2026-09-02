// corpus: reject/r149-switch-duplicate-declaration
// purpose: Rejects two declarations of one name in one switch body.
// exercises: switch-body-scope, duplicate-declaration
// questions: §67
// tsc: rejects TS2451
// expected-error: S017 at the second declaration, with a message that names the switch
export function main(): void {
  const selected: i32 = 1;
  switch (selected) {
    case 0:
      let value: i32 = 1;
      break;
    case 1:
      let value: i32 = 2;
      value = 3;
      break;
  }
}
