// corpus: reject/r66-coroutine-step-layout-too-large
// purpose: Rejects a coroutine step result whose done-plus-value layout exceeds the limit.
// exercises: IterResult layout, FixedArray yield type
// questions: Q3, Q11
// expected-error: S100 at the next member

function* huge(): Generator<FixedArray<u8, 2147483647>> {
  while (true) {}
}

export function main(): void {
  const step = huge().next();
  print(`${step.done}`);
}
