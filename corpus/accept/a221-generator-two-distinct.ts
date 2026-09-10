// corpus: accept/a221-generator-two-distinct
// purpose: Stores two generators and drives each one in turn.
// observable: each stored generator keeps its own suspended state.
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
  step(): i32 {
    const result = this.source.next();
    if (result.done) {
      return -1;
    }
    return result.value;
  }
}

export function main(): void {
  const low: Holder = new Holder(upTo(1, 3));
  const high: Holder = new Holder(upTo(10, 12));
  print(`low ${low.step()} high ${high.step()}`);
  print(`low ${low.step()} high ${high.step()}`);
  print(`high ${high.step()} low ${low.step()}`);
  print(`low ${low.step()} high ${high.step()}`);
}
