// corpus: accept/a215-user-view-method-names
// purpose: Accepts user methods named keys, values, and entries.
// exercises: user-class-method, generator, for-of, view-name
// questions: Q30, compiler section 103
// tsc: accepts; js-comparable: yes
function* upTo(first: i32, last: i32): Generator<i32> {
  for (let value: i32 = first; value <= last; value += 1) {
    yield value;
  }
}

class Bag {
  first: i32;
  last: i32;
  constructor(first: i32, last: i32) {
    this.first = first;
    this.last = last;
  }
  keys(): Generator<i32> {
    return upTo(this.first, this.last);
  }
  values(): Generator<i32> {
    return upTo(this.first + 10, this.last + 10);
  }
  entries(): Generator<i32> {
    return upTo(this.first + this.last, this.first + this.last + 1);
  }
}

export function main(): void {
  const bag: Bag = new Bag(1, 3);

  let keys: string = "";
  for (const key of bag.keys()) {
    keys += `${key},`;
  }
  print(`keys ${keys}`);

  let values: string = "";
  for (const value of bag.values()) {
    values += `${value},`;
  }
  print(`values ${values}`);

  let entries: string = "";
  for (const entry of bag.entries()) {
    entries += `${entry},`;
  }
  print(`entries ${entries}`);

  const cursor = bag.values();
  let stepped: string = "";
  while (true) {
    const step = cursor.next();
    if (step.done) {
      break;
    }
    stepped += `${step.value},`;
  }
  print(`stepped ${stepped}`);
}
