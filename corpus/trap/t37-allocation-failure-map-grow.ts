// corpus: trap/t37-allocation-failure-map-grow
// purpose: Injects allocation failure while Map.set creates ordered backing storage.
// exercises: allocation-failure, Map.set, growth
// questions: Context::trap is first-wins, so the injected Context message is the reported message and later assocops fallback messages cannot replace it
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  const values: Map<i32, i32> = new Map<i32, i32>();
  print("before");
  values.set(1, 7);
  print(`${values.size}`);
}
