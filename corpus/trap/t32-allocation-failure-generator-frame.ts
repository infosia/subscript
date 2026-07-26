// corpus: trap/t32-allocation-failure-generator-frame
// purpose: Injects allocation failure when a generator frame is created.
// exercises: allocation-failure, generator-frame
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

function* values(): Generator<i32> {
  yield 1;
}

export function main(): void {
  print("before");
  const generator: Generator<i32> = values();
  print(`${generator.next().value}`);
}
