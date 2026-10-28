// corpus: accept/a393-local-function-in-a-class-body
// purpose: A local function in a method keeps the class body for member access, so it reads private members of its class.
// exercises: local-functions, private-members, private-names
// questions: compiler.md §207.1 rule 2
// tsc: accepts; js-comparable: yes
class Counter {
  private count: i32 = 0;
  #step: i32 = 1;

  constructor(start: i32, step: i32) {
    this.count = start;
    this.#step = step;
  }

  static total(xs: Counter[]): i32 {
    function countOf(c: Counter): i32 {
      return c.count + c.#step;
    }
    let s: i32 = 0;
    for (const c of xs) {
      s += countOf(c);
    }
    return s;
  }

  bumpAll(xs: Counter[]): void {
    function bump(c: Counter): void {
      c.count += c.#step;
    }
    for (const c of xs) {
      bump(c);
    }
  }
}

export function main(): void {
  const xs = [new Counter(1, 2), new Counter(10, 3)];
  print(`${Counter.total(xs)}`);
  xs[0].bumpAll(xs);
  print(`${Counter.total(xs)}`);
}
