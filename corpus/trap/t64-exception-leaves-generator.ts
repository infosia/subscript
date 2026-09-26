// corpus: trap/t64-exception-leaves-generator
// purpose: An exception that leaves a generator body becomes the uncaught-exception trap at the body, so no handler of the consumer catches it.
// exercises: throw, generator, generator-driven-for-of, uncaught-exception, array-for-each, try-catch
// questions: Q9, Q33
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in `numbers`
function* numbers(limit: i32): Generator<i32> {
  for (let i: i32 = 0; i < limit; i++) {
    if (i === 2) {
      throw new TypeError(`step ${i} is out of range`);
    }
    yield i;
  }
}

export function main(): void {
  const limits: i32[] = [4];
  try {
    // The forEach call is a raise site with a handler, so an exception
    // that left the generator body would reach this catch.
    limits.forEach((limit: i32): void => {
      for (const n of numbers(limit)) {
        print(`n=${n}`);
      }
    });
  } catch {
    print("caught");
  }
  print("unreached");
}
