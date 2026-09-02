// corpus: reject/r164-duplicate-static-member-name
// purpose: Rejects two static members with the same name.
// exercises: static-namespace, duplicate-static-member
// questions: §71
// tsc: rejects TS2300
// expected-error: S017 at the second declaration
class C {
  static value: i32 = 1;
  static value(): i32 {
    return 2;
  }
}

export function main(): void {}
