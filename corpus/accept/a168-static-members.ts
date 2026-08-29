// corpus: accept/a168-static-members
// purpose: Runs static fields, methods, accessors, namespaces, and ordered initializers.
// exercises: static-field, static-method, static-accessor, static-namespace, module-initializer
// questions: §71
// tsc: accepts; js-comparable: yes

function initialize(value: i32, label: string): i32 {
  print(label);
  return value;
}

const seed: i32 = initialize(3, "seed");
print("before-class");

class Counter {
  static value: i32 = initialize(seed, "static-value");
  value: i32 = 10;
  static next: i32 = Counter.value + 2;
  static readonly fixed: i32 = Counter.next + 2;

  static add(amount: i32): i32 {
    Counter.value += amount;
    return Counter.value;
  }

  static get doubled(): i32 {
    return Counter.value * 2;
  }

  static set doubled(value: i32) {
    Counter.value = value / 2;
  }
}

print(`after-counter:${Counter.next}`);

class Point {
  static origin: i32 = Counter.next;
  x: i32 = 0;

  static readOrigin(): i32 {
    return Point.origin;
  }
}

print(`after-point:${Point.origin}`);

export function main(): void {
  const counter: Counter = new Counter();
  print(`${Counter.value}:${counter.value}`);
  print(`${Counter.add(4)}`);
  print(`${Counter.doubled}`);
  Counter.doubled = 20;
  print(`${Counter.value}:${Counter.fixed}`);
  print(`${Point.readOrigin()}`);
}
