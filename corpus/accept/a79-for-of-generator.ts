// corpus: accept/a79-for-of-generator
// purpose: P22 drives C8 generators through the already-contracted next() protocol.
// observable: for-of and hand-written next() print the same sequence.
// exercises: for-of-generator, coroutine-next, iterator-result
// questions: Q30, Q11
// tsc: accepts; js-comparable: yes
function* values(): Generator<i32> {
  yield 3;
  yield 5;
  yield 8;
}

export function main(): void {
  for (const value of values()) {
    print(`for-of:${value}`);
  }

  const generator: Generator<i32> = values();
  let step = generator.next();
  while (!step.done) {
    print(`manual:${step.value}`);
    step = generator.next();
  }
}
