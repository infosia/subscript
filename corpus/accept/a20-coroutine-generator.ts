// corpus: accept/a20-coroutine-generator
// purpose: Drives a generator explicitly until completion.
// exercises: generator, yield, host-driven-coroutine
// questions: Q1, Q11, Q12

function* sequence(limit: i32) {
  for (let value: i32 = 1; value <= limit; value += 1) {
    yield value;
  }
}

export function main(): void {
  const generator = sequence(4);
  let total: i32 = 0;
  while (true) {
    const step = generator.next();
    if (step.done) {
      break;
    }
    total += step.value;
  }
  print(`${total}`);
}
