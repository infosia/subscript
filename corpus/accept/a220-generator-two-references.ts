// corpus: accept/a220-generator-two-references
// purpose: Drives one generator through a local and through a field in turn.
// observable: the two names share one suspended state, so the values ascend.
// exercises: generator, escaping-value, class-field, generator-identity
// questions: Q30, compiler section 106
// tsc: accepts; js-comparable: yes
function* upTo(first: i32, last: i32): Generator<i32> {
  for (let value: i32 = first; value <= last; value += 1) {
    yield value;
  }
}

class Holder {
  source: Generator<i32>;
  constructor(source: Generator<i32>) {
    this.source = source;
  }
}

function step(source: Generator<i32>): i32 {
  const result = source.next();
  if (result.done) {
    return -1;
  }
  return result.value;
}

export function main(): void {
  const local: Generator<i32> = upTo(1, 4);
  const holder: Holder = new Holder(local);
  print(`local ${step(local)}`);
  print(`field ${step(holder.source)}`);
  print(`local ${step(local)}`);
  print(`field ${step(holder.source)}`);
  print(`local ${step(local)}`);
}
