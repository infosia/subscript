// corpus: accept/a176-compound-through-accessor
// purpose: Runs compound assignments and updates through accessors and a class index signature.
// observable: Read-then-write results and single-evaluation counters print after each form.
// exercises: instance-accessor, static-accessor, class-index-signature, compound-assignment, increment, decrement, synthetic-local
// questions: R39.3, §82.1, C10, C12
// tsc: accepts; js-comparable: no C10: JavaScript reads numeric properties instead of the declared index accessors.

class Counter {
  value: i32;
  static totalValue: i32 = 4;

  constructor(value: i32) {
    this.value = value;
  }

  get v(): i32 {
    return this.value;
  }

  set v(value: i32) {
    this.value = value;
  }

  static get total(): i32 {
    return Counter.totalValue;
  }

  static set total(value: i32) {
    Counter.totalValue = value;
  }
}

class Values {
  [index: u32]: i32;
  data: i32[] = [8];

  get(index: u32): i32 {
    return this.data[index as i32];
  }

  set(index: u32, value: i32): void {
    this.data[index as i32] = value;
  }
}

class Label {
  value: string = "sub";

  get text(): string {
    return this.value;
  }

  set text(value: string) {
    this.value = value;
  }
}

class ReceiverSource {
  static calls: i32 = 0;
  static counter: Counter = new Counter(20);

  static next(): Counter {
    ReceiverSource.calls++;
    return ReceiverSource.counter;
  }
}

class IndexSource {
  static calls: i32 = 0;

  static next(): u32 {
    IndexSource.calls++;
    return 0;
  }
}

function maybe(keep: boolean): Counter | null {
  return keep ? new Counter(1) : null;
}

export function main(): void {
  const counter: Counter = new Counter(3);
  counter.v += 2;
  counter.v -= 1;
  counter.v *= 3;
  counter.v++;
  --counter.v;
  for (let i: i32 = 0; i < 3; counter.v++) {
    i++;
  }
  print(`instance:${counter.v}`);

  Counter.total += 6;
  Counter.total++;
  print(`static:${Counter.total}`);

  const values: Values = new Values();
  const zero: u32 = 0;
  values[zero] += 4;
  --values[zero];
  print(`index:${values[zero]}`);

  const label: Label = new Label();
  label.text += "script";
  print(label.text);

  ReceiverSource.next().v += 2;
  print(`receiver:${ReceiverSource.calls}:${ReceiverSource.counter.v}`);

  values[IndexSource.next()] += 3;
  print(`index-call:${IndexSource.calls}:${values[zero]}`);

  const fb: Counter = new Counter(1);
  let j: i32 = 0;
  for (j = 0; j < 3 && (maybe(false) ?? fb).v > 0; j++) { }
  print(`empty:${j}`);
}
