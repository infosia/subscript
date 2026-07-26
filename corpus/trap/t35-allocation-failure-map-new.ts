// corpus: trap/t35-allocation-failure-map-new
// purpose: Injects allocation failure while creating a Map header.
// exercises: allocation-failure, Map, header
// questions: Context::trap is first-wins, so the injected Context message is the reported message and later assocops fallback messages cannot replace it
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  print("before");
  const values: Map<i32, i32> = new Map<i32, i32>();
  print(`${values.size}`);
}
