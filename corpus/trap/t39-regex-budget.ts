// corpus: trap/t39-regex-budget
// feature: regex
// purpose: Budget exhaustion is a trap, never a no-match result.
// exercises: RegExp.test, deterministic Context budget, cross-tier trap
// expected-trap: regex-budget at the test call

export function main(): void {
  print("before regex");
  print(`${/(a+)+$/.test("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab")}`);
  print("after regex");
}
