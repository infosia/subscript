// corpus: accept/a222-generator-field-replaced
// purpose: Replaces a generator field after the first generator is part read.
// observable: the new generator starts at its own first value, and the old one keeps its place.
// exercises: generator, escaping-value, class-field, field-assignment
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
  const first: Generator<i32> = upTo(1, 5);
  const holder: Holder = new Holder(first);
  print(`before ${step(holder.source)} ${step(holder.source)}`);

  holder.source = upTo(20, 22);
  print(`after ${step(holder.source)} ${step(holder.source)}`);
  print(`old ${step(first)}`);
}
