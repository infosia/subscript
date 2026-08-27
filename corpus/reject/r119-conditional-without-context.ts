// corpus: reject/r119-conditional-without-context
// purpose: Keeps the directional branch rule for a conditional with no contextual type.
// exercises: conditional-expression, no-context, directional-branch-assignability
// questions: R18
// tsc: accepts
// expected-error: S100 at the null else branch

class BranchValue {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const flag: boolean = true;
  const value = flag ? new BranchValue(7) : null;
  print(`${value !== null}`);
}
