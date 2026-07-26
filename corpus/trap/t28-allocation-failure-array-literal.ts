// corpus: trap/t28-allocation-failure-array-literal
// purpose: Injects allocation failure at a dynamic-array literal header.
// exercises: allocation-failure, array-literal
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  print("before");
  const values: i32[] = [1, 2];
  print(`${values.length}`);
}
