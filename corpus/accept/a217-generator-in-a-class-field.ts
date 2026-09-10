// corpus: accept/a217-generator-in-a-class-field
// interpreter: no — the interpreter holds a Generator as a coroutine frame, which it cannot pack into a class field
// purpose: Stores a Generator in a class field and drives it two ways.
// exercises: generator, escaping-value, class-field, for-of
// questions: Q30, compiler section 103
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
  print(`stored ${holder.step()} ${holder.step()} ${holder.step()}`);

  const field: Holder = new Holder(upTo(3, 5));
  let out: string = "";
  for (const value of field.source) {
    out += `${value},`;
  }
  print(`field ${out}`);
}
