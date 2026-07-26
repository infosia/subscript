// corpus: trap/t38-allocation-failure-set-grow
// purpose: Injects allocation failure while Set.add creates ordered backing storage.
// exercises: allocation-failure, Set.add, growth
// questions: Context::trap is first-wins, so the injected Context message is the reported message and later assocops fallback messages cannot replace it
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  const values: Set<i32> = new Set<i32>();
  print("before");
  values.add(7);
  print(`${values.size}`);
}
