// corpus: accept/a394-interface-constraint/main
// purpose: An interface is a constraint: a generic function, method, and class over T extends I call the members of each class directly.
// exercises: interface, implements, generic-constraint, module-import
// questions: compiler.md §208
// tsc: accepts; js-comparable: yes
import { Circle, Square } from "./shapes";
import type { Shape, Tagged } from "./shapes";

interface Counter {
  next(): i32;
}

class Steps implements Counter {
  n: i32 = 0;
  next(): i32 {
    this.n += 2;
    return this.n;
  }
}

function total<T extends Shape>(xs: T[]): f64 {
  let t: f64 = 0;
  for (const x of xs) {
    t += x.area();
  }
  return t;
}

function describe<T extends Shape>(x: T, k: f64): string {
  return `${x.name}:${x.scaled(k)}`;
}

function tagOf<T extends Tagged>(x: T): i32 {
  return x.tag();
}

class Pair<T extends Shape> {
  a: T;
  b: T;
  constructor(a: T, b: T) {
    this.a = a;
    this.b = b;
  }
  sum(): f64 {
    return this.a.area() + this.b.area();
  }
}

function drain<C extends Counter>(c: C, times: i32): i32 {
  let last: i32 = 0;
  for (let i: i32 = 0; i < times; i += 1) {
    last = c.next();
  }
  return last;
}

export function main(): void {
  const squares = [new Square(1), new Square(2)];
  const circles = [new Circle(1)];
  print(`${total(squares)} ${total(circles)} ${total<Square>([])}`);
  print(`${describe(new Square(3), 2)} ${describe(new Circle(2), 0.5)}`);
  print(`${tagOf(new Square(1))}`);
  const p = new Pair(new Circle(1), new Circle(2));
  print(`${p.sum()}`);
  print(`${drain(new Steps(), 3)}`);
}
