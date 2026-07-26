// corpus: trap/t29-allocation-failure-push-grow
// purpose: Injects allocation failure when `push` grows element storage.
// exercises: allocation-failure, array-push, growth
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  const values: i32[] = [];
  print("before");
  values.push(7);
  print(`${values.length}`);
}
