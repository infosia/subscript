// corpus: accept/a225-generator-exhausted-in-storage
// purpose: Reads an exhausted generator again from the field that holds it.
// observable: the field answers done for every read after the last value.
// exercises: generator, escaping-value, class-field, generator-exhaustion
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
  const holder: Holder = new Holder(upTo(1, 2));
  print(`drive ${holder.step()} ${holder.step()} ${holder.step()}`);
  print(`again ${holder.step()} ${holder.step()}`);

  let out: string = "";
  for (const value of holder.source) {
    out += `${value},`;
  }
  print(`for-of "${out}"`);
}
