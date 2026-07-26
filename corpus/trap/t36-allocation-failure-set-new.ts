// corpus: trap/t36-allocation-failure-set-new
// purpose: Injects allocation failure while creating a Set header.
// exercises: allocation-failure, Set, header
// questions: Context::trap is first-wins, so the injected Context message is the reported message and later assocops fallback messages cannot replace it
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  print("before");
  const values: Set<i32> = new Set<i32>();
  print(`${values.size}`);
}
